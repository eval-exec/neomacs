//! Text window row lifecycle and append surface construction.
//!
//! This module holds helpers that translate text-window geometry and chrome
//! reservation policy into a generic `DisplayRowAppendSurface`, then install
//! rendered rows, cursor effects, retry metadata, and final window snapshots.

use crate::display_row::append_context::{
    DisplayRowAppendArea, DisplayRowAppendSurface, RightEdgeMarkerColumn,
};
use crate::display_row::builder::DisplayTabPolicy;
use crate::display_row::geometry::{
    DisplayRowFlags, DisplayRowGeometryState, DisplayRowLimit, DisplayRowYPositions,
};
use crate::display_row::source_render::TextRowOutputRenderState;
use crate::display_row::special_glyphs::{
    TextWindowRightEdgeMarkers, TextWindowTerminalRightBorder,
    install_text_window_terminal_right_border,
};
use crate::display_row::walk_state::{
    HitRowRangeTracker, next_window_start_for_partially_visible_point_row,
    next_window_start_for_point_line_continuation, next_window_start_from_visible_rows,
    visible_rows_below,
};
use crate::display_status_line::ChromeRowRenderServices;
use crate::hit_test::HitRow;
use crate::neovm_bridge::{ForwardScrollMeasurement, LayoutBufferView, RustBufferAccess};
use crate::scroll_policy::{
    ForwardScroll, ScrollPolicy, count_lines_bounded, last_usable_row, line_start_above,
    line_start_below,
};
use crate::types::WindowParams;
use crate::window_output::{
    DisplayTextRowBegin, TextWindowBegin, TextWindowBodyOutputInstall, TextWindowCursorEffects,
    TextWindowOutputTarget, TextWindowPendingRowFinish, TextWindowRedisplayPositions,
    WindowOutputEmitter, begin_text_window_output_and_row, close_text_window_output,
    finish_pending_text_window_row, install_text_window_body_output,
    install_text_window_cursor_effects,
};
use neomacs_display_protocol::effect_config::EffectsConfig;
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::LispCharPos1;
use neovm_core::emacs_core::Context;
use neovm_core::window::{
    DisplayRowSnapshot, FrameId, PresentedWindowRegions, WindowDisplaySnapshot, WindowId,
    geometry::CellOrigin,
};

use crate::coords::layout_i64_char_pos_to_lisp_char_pos;
use crate::display_cursor::{
    CapturedTextWindowCursorPublishContext, CapturedTextWindowCursorPublishOutcome,
    CursorCaptureState, VisualTextWindowCursorPublishContext, VisualTextWindowCursorPublishSummary,
};
use crate::display_face_policy::EffectiveWindowDefaultFace;

/// Columns at the terminal right edge that body text must not consume.
///
/// Keeping the combinations as an enum makes the layout budget exhaustive:
/// adding a marker or border cannot silently leave its width out of the text
/// extent.  GNU applies the same policy by subtracting the continuation glyph
/// width from `last_visible_x` before producing body glyphs (`init_iterator`,
/// xdisp.c), and then separately reserving a non-rightmost TTY border.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextWindowRightEdgeReservation {
    None,
    EdgeMarker,
    TerminalBorder,
    EdgeMarkerAndTerminalBorder,
}

impl TextWindowRightEdgeReservation {
    const fn for_columns(reserve_edge_marker: bool, reserve_terminal_border: bool) -> Self {
        match (reserve_edge_marker, reserve_terminal_border) {
            (false, false) => Self::None,
            (true, false) => Self::EdgeMarker,
            (false, true) => Self::TerminalBorder,
            (true, true) => Self::EdgeMarkerAndTerminalBorder,
        }
    }

    const fn column_count(self) -> usize {
        match self {
            Self::None => 0,
            Self::EdgeMarker | Self::TerminalBorder => 1,
            Self::EdgeMarkerAndTerminalBorder => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextWindowAppendSurfaceRequest<'a> {
    content_x: f32,
    text_width: f32,
    line_number_width: f32,
    right_edge_reservation: TextWindowRightEdgeReservation,
    char_width: f32,
    tab_width: i32,
    tab_stop_list: &'a [i32],
}

impl<'a> TextWindowAppendSurfaceRequest<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        content_x: f32,
        text_width: f32,
        line_number_width: f32,
        reserve_right_border_col: bool,
        reserve_right_special_col: bool,
        char_width: f32,
        tab_width: i32,
        tab_stop_list: &'a [i32],
    ) -> Self {
        Self {
            content_x,
            text_width,
            line_number_width,
            right_edge_reservation: TextWindowRightEdgeReservation::for_columns(
                reserve_right_special_col,
                reserve_right_border_col,
            ),
            char_width,
            tab_width,
            tab_stop_list,
        }
    }

    fn reserved_width(self) -> f32 {
        self.char_width * self.right_edge_reservation.column_count() as f32
    }

    fn append_width(self) -> f32 {
        (self.text_width - self.line_number_width - self.reserved_width()).max(self.char_width)
    }

    fn right_edge_marker_column(self) -> RightEdgeMarkerColumn {
        match self.right_edge_reservation {
            TextWindowRightEdgeReservation::EdgeMarker
            | TextWindowRightEdgeReservation::EdgeMarkerAndTerminalBorder => {
                RightEdgeMarkerColumn::Reserved
            }
            TextWindowRightEdgeReservation::None
            | TextWindowRightEdgeReservation::TerminalBorder => RightEdgeMarkerColumn::NotReserved,
        }
    }

    pub(crate) fn into_surface(self) -> DisplayRowAppendSurface {
        let right_edge_marker_column = self.right_edge_marker_column();
        DisplayRowAppendSurface::new(
            DisplayRowAppendArea::new(
                self.content_x,
                self.append_width(),
                self.text_width,
                self.line_number_width,
            ),
            DisplayTabPolicy::from_tab_width_and_stops(
                self.content_x,
                self.tab_width,
                self.tab_stop_list,
            ),
        )
        .with_right_edge_marker_column(right_edge_marker_column)
    }
}

pub(crate) struct TextWindowCursorEffectsRequest {
    window_id: i64,
    effects: Option<EffectsConfig>,
}

impl TextWindowCursorEffectsRequest {
    pub(crate) fn new(window_id: i64, effects: Option<EffectsConfig>) -> Self {
        Self { window_id, effects }
    }

    pub(crate) fn install_and_apply(self, output: TextWindowOutputTarget<'_>) -> bool {
        let Some(effects) = self.effects else {
            return false;
        };
        install_text_window_cursor_effects(
            output,
            TextWindowCursorEffects {
                window_id: self.window_id,
                effects,
            },
        );
        true
    }
}

pub(crate) struct TextWindowTerminalRightBorderRequest {
    ch: char,
    face_name: &'static str,
    char_width: f32,
}

impl TextWindowTerminalRightBorderRequest {
    pub(crate) fn new(char_width: f32) -> Self {
        Self {
            ch: '|',
            face_name: "vertical-border",
            char_width,
        }
    }

    pub(crate) fn install_and_apply(
        self,
        mut output: TextWindowOutputTarget<'_>,
        render_services: ChromeRowRenderServices<'_, '_>,
        effective_default_face: &EffectiveWindowDefaultFace,
    ) -> FaceId {
        install_text_window_terminal_right_border(
            output.builder(),
            TextWindowTerminalRightBorder {
                ch: self.ch,
                face_name: self.face_name,
                char_width: self.char_width,
            },
            render_services,
            effective_default_face,
        )
    }
}

pub(crate) struct TextWindowBeginRequest {
    frame_id: FrameId,
    window_id: WindowId,
    display_text_row_base: usize,
    text_area_left: f32,
    window_top: f32,
    output_window_id: u64,
    output_rows: usize,
    output_cols: usize,
    bounds: neomacs_display_protocol::types::Rect,
    text_bounds: neomacs_display_protocol::types::Rect,
    text_clip_bounds: neomacs_display_protocol::types::Rect,
    selected: bool,
    first_row: DisplayTextRowBegin,
}

pub(crate) struct TextWindowTailFinalizeRequest<'a> {
    context: TextWindowTailFinalizeContext<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct TextWindowTailFinalizeContext<'a> {
    params: &'a WindowParams,
    text: &'a [u8],
    display_text_row_base: usize,
    text_area_left: f32,
    window_top: f32,
    text_y: f32,
    text_height: f32,
    char_w: f32,
    char_h: f32,
    window_start: i64,
    point_charpos: i64,
    charpos: i64,
    point_is_visible_eob: bool,
    row_limit: DisplayRowLimit,
}

pub(crate) struct TextWindowTailFinalizeState<'a, 'emit> {
    cursor_info: &'emit mut CursorCaptureState,
    row_geometry: &'a DisplayRowGeometryState,
    row_y_positions: &'a DisplayRowYPositions,
    hit_row_range: &'emit mut HitRowRangeTracker,
    hit_rows: &'emit mut Vec<HitRow>,
    output_render: TextRowOutputRenderState<'emit>,
}

pub(crate) struct TextWindowBodyInstallRequest<'a> {
    context: TextWindowBodyInstallRenderContext<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct TextWindowBodyInstallRenderContext<'a> {
    window_id: u64,
    window_start: i64,
    text_start_byte: usize,
    byte_idx: usize,
    reserve_right_special_col: bool,
    reserve_right_border_col: bool,
    display_text_row_base: usize,
    output_cols: usize,
    row_flags: &'a DisplayRowFlags,
    right_edge_face_id: FaceId,
    right_edge_face: &'a crate::neovm_bridge::ResolvedFace,
    char_w: f32,
}

pub(crate) struct TextWindowBodyInstallState<'emit, 'output, 'face> {
    output: TextWindowOutputTarget<'output>,
    output_emitter: &'output mut WindowOutputEmitter,
    render_services: ChromeRowRenderServices<'emit, 'face>,
}

pub(crate) struct TextWindowVisibilityRetryRequest<'a, 'buf, B: LayoutBufferView> {
    rows: &'a [DisplayRowSnapshot],
    window_start: i64,
    accessible_start: i64,
    accessible_end: i64,
    point_charpos: i64,
    charpos: i64,
    point_is_visible_eob: bool,
    text_area_top: i64,
    text_area_bottom: i64,
    scroll_policy: ScrollPolicy,
    scroll_margin: i64,
    buf_access: &'a RustBufferAccess<'buf, B>,
}

pub(crate) struct TextWindowFinishRequest {
    cell_origin: CellOrigin,
    regions: PresentedWindowRegions,
    mode_line_height: i64,
    header_line_height: i64,
    tab_line_height: i64,
}

pub(crate) struct TextWindowFinishState<'a> {
    output: TextWindowOutputTarget<'a>,
    output_emitter: WindowOutputEmitter,
    evaluator: &'a mut Context,
    hit_rows: Vec<HitRow>,
}

pub(crate) struct TextWindowFinishOutput {
    snapshot: WindowDisplaySnapshot,
}

impl TextWindowFinishOutput {
    pub(crate) fn into_snapshot(self) -> WindowDisplaySnapshot {
        self.snapshot
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextWindowVisibilityRetryOutcome {
    visible_end_lisp: Option<LispCharPos1>,
    visible_progress: i64,
    point_beyond_visible_span: bool,
    scroll_down_window_start: Option<i64>,
    point_row_window_start: Option<i64>,
    point_line_window_start: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextWindowTailFinalizeOutcome {
    cursor_requested: bool,
    cursor_publish_status: TextWindowCursorPublishStatus,
    visual_cursor_summary: VisualTextWindowCursorPublishSummary,
    pending_row_finished: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextWindowCursorPublishStatus {
    NotRequested,
    MissingCapture,
    NoWindowCursor,
    Clipped,
    Published,
}

impl From<CapturedTextWindowCursorPublishOutcome> for TextWindowCursorPublishStatus {
    fn from(outcome: CapturedTextWindowCursorPublishOutcome) -> Self {
        match outcome {
            CapturedTextWindowCursorPublishOutcome::NoWindowCursor => Self::NoWindowCursor,
            CapturedTextWindowCursorPublishOutcome::Clipped => Self::Clipped,
            CapturedTextWindowCursorPublishOutcome::Published => Self::Published,
        }
    }
}

impl TextWindowTailFinalizeOutcome {
    #[cfg(test)]
    pub(crate) fn cursor_requested(self) -> bool {
        self.cursor_requested
    }

    #[cfg(test)]
    pub(crate) fn cursor_published(self) -> bool {
        matches!(
            self.cursor_publish_status,
            TextWindowCursorPublishStatus::Published
        )
    }

    #[cfg(test)]
    pub(crate) fn cursor_publish_status(self) -> TextWindowCursorPublishStatus {
        self.cursor_publish_status
    }

    #[cfg(test)]
    pub(crate) fn visual_cursor_summary(self) -> VisualTextWindowCursorPublishSummary {
        self.visual_cursor_summary
    }

    #[cfg(test)]
    pub(crate) fn pending_row_finished(self) -> bool {
        self.pending_row_finished
    }
}

impl<'a, 'emit> TextWindowTailFinalizeState<'a, 'emit> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        cursor_info: &'emit mut CursorCaptureState,
        row_geometry: &'a DisplayRowGeometryState,
        row_y_positions: &'a DisplayRowYPositions,
        hit_row_range: &'emit mut HitRowRangeTracker,
        hit_rows: &'emit mut Vec<HitRow>,
        output_render: TextRowOutputRenderState<'emit>,
    ) -> Self {
        Self {
            cursor_info,
            row_geometry,
            row_y_positions,
            hit_row_range,
            hit_rows,
            output_render,
        }
    }
}

impl<'emit, 'output, 'face> TextWindowBodyInstallState<'emit, 'output, 'face> {
    pub(crate) fn new(
        output: TextWindowOutputTarget<'output>,
        output_emitter: &'output mut WindowOutputEmitter,
        render_services: ChromeRowRenderServices<'emit, 'face>,
    ) -> Self {
        Self {
            output,
            output_emitter,
            render_services,
        }
    }
}

impl<'a> TextWindowFinishState<'a> {
    pub(crate) fn new(
        output: TextWindowOutputTarget<'a>,
        output_emitter: WindowOutputEmitter,
        evaluator: &'a mut Context,
        hit_rows: Vec<HitRow>,
    ) -> Self {
        Self {
            output,
            output_emitter,
            evaluator,
            hit_rows,
        }
    }

    fn finish_snapshot(
        self,
        cell_origin: CellOrigin,
        regions: PresentedWindowRegions,
        mode_line_height: i64,
        header_line_height: i64,
        tab_line_height: i64,
    ) -> (Vec<HitRow>, WindowDisplaySnapshot) {
        close_text_window_output(self.output);
        let snapshot = self.output_emitter.finish_snapshot_with_geometry(
            self.evaluator,
            cell_origin,
            regions,
            mode_line_height,
            header_line_height,
            tab_line_height,
        );
        (self.hit_rows, snapshot)
    }
}

impl TextWindowBeginRequest {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        frame_id: FrameId,
        window_id: WindowId,
        display_text_row_base: usize,
        text_area_left: f32,
        window_top: f32,
        output_window_id: u64,
        output_rows: usize,
        output_cols: usize,
        bounds: neomacs_display_protocol::types::Rect,
        text_bounds: neomacs_display_protocol::types::Rect,
        text_clip_bounds: neomacs_display_protocol::types::Rect,
        selected: bool,
        first_row: DisplayTextRowBegin,
    ) -> Self {
        Self {
            frame_id,
            window_id,
            display_text_row_base,
            text_area_left,
            window_top,
            output_window_id,
            output_rows,
            output_cols,
            bounds,
            text_bounds,
            text_clip_bounds,
            selected,
            first_row,
        }
    }

    pub(crate) fn begin_and_apply(
        self,
        output: TextWindowOutputTarget<'_>,
        evaluator: &mut Context,
    ) -> WindowOutputEmitter {
        let mut output_emitter = WindowOutputEmitter::new_speculative(
            self.frame_id,
            self.window_id,
            self.display_text_row_base,
            self.text_area_left,
            self.window_top,
        );
        output_emitter.begin_update(evaluator);
        begin_text_window_output_and_row(
            output,
            &mut output_emitter,
            evaluator,
            TextWindowBegin {
                window_id: self.output_window_id,
                rows: self.output_rows,
                cols: self.output_cols,
                bounds: self.bounds,
                text_bounds: self.text_bounds,
                text_clip_bounds: self.text_clip_bounds,
                selected: self.selected,
                first_row: self.first_row,
            },
        );
        output_emitter
    }

    pub(crate) fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.window_id
    }
}

impl<'a> TextWindowTailFinalizeContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        params: &'a WindowParams,
        text: &'a [u8],
        display_text_row_base: usize,
        text_area_left: f32,
        window_top: f32,
        text_y: f32,
        text_height: f32,
        char_w: f32,
        char_h: f32,
        window_start: i64,
        point_charpos: i64,
        charpos: i64,
        point_is_visible_eob: bool,
        row_limit: DisplayRowLimit,
    ) -> Self {
        Self {
            params,
            text,
            display_text_row_base,
            text_area_left,
            window_top,
            text_y,
            text_height,
            char_w,
            char_h,
            window_start,
            point_charpos,
            charpos,
            point_is_visible_eob,
            row_limit,
        }
    }
}

impl<'a> TextWindowTailFinalizeRequest<'a> {
    pub(crate) fn new(context: TextWindowTailFinalizeContext<'a>) -> Self {
        Self { context }
    }

    pub(crate) fn finalize_and_apply(
        self,
        state: TextWindowTailFinalizeState<'_, '_>,
    ) -> TextWindowTailFinalizeOutcome {
        let TextWindowTailFinalizeState {
            cursor_info,
            row_geometry,
            row_y_positions,
            hit_row_range,
            hit_rows,
            output_render,
        } = state;
        let context = self.context;

        let cursor_requested = context.point_charpos >= context.window_start
            && (context.point_charpos <= context.charpos || context.point_is_visible_eob);
        let initial_cursor_publish_status = if cursor_requested {
            TextWindowCursorPublishStatus::MissingCapture
        } else {
            TextWindowCursorPublishStatus::NotRequested
        };

        let (cursor_publish_status, pending_row_finished, visual_cursor_summary) =
            output_render.with_output_target_parts(|mut output, output_emitter, _evaluator| {
                let mut cursor_publish_status = initial_cursor_publish_status;
                if cursor_requested {
                    if let Some(cursor) = cursor_info.captured() {
                        let cursor_row_metrics = output_emitter.row_metrics().to_vec();
                        cursor_publish_status = CapturedTextWindowCursorPublishContext::new(
                            context.params,
                            context.text,
                            context.display_text_row_base,
                            context.text_area_left,
                            context.window_top,
                            context.text_y,
                            context.text_height,
                            context.char_w,
                            context.char_h,
                            context.point_charpos,
                            context.point_is_visible_eob,
                        )
                        .publish_captured_cursor(
                            cursor,
                            &cursor_row_metrics,
                            row_geometry.row_metrics_snapshot(context.display_text_row_base),
                            output.reborrow(),
                            output_emitter,
                        )
                        .into();
                    } else {
                        tracing::debug!(
                            "layout_window_rust: no explicit cursor capture for point={} window_start={} charpos_end={}",
                            context.point_charpos,
                            context.window_start,
                            context.charpos
                        );
                    }
                }

                let pending_row_finished = finish_pending_text_window_row(
                    output.reborrow(),
                    output_emitter,
                    TextWindowPendingRowFinish {
                        row_geometry,
                        source_exhausted: context.charpos
                            >= context.params.accessible_end_charpos().get(),
                        row_limit: context.row_limit,
                        row_y_positions,
                        text_y: context.text_y,
                        char_height: context.char_h,
                        charpos: context.charpos,
                        hit_row_range,
                        hit_rows,
                    },
                );

                let visual_cursor_summary = VisualTextWindowCursorPublishContext::new(
                    context.params,
                    context.text_area_left,
                    context.window_top,
                    context.text_y,
                    context.text_height,
                    context.char_w,
                )
                .publish_visual_cursors(output.reborrow(), output_emitter);
                (
                    cursor_publish_status,
                    pending_row_finished,
                    visual_cursor_summary,
                )
            });

        TextWindowTailFinalizeOutcome {
            cursor_requested,
            cursor_publish_status,
            visual_cursor_summary,
            pending_row_finished,
        }
    }
}

impl<'a> TextWindowBodyInstallRenderContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        window_id: u64,
        window_start: i64,
        text_start_byte: usize,
        byte_idx: usize,
        reserve_right_special_col: bool,
        reserve_right_border_col: bool,
        display_text_row_base: usize,
        output_cols: usize,
        row_flags: &'a DisplayRowFlags,
        right_edge_face_id: FaceId,
        right_edge_face: &'a crate::neovm_bridge::ResolvedFace,
        char_w: f32,
    ) -> Self {
        Self {
            window_id,
            window_start,
            text_start_byte,
            byte_idx,
            reserve_right_special_col,
            reserve_right_border_col,
            display_text_row_base,
            output_cols,
            row_flags,
            right_edge_face_id,
            right_edge_face,
            char_w,
        }
    }
}

impl<'a> TextWindowBodyInstallRequest<'a> {
    pub(crate) fn new(context: TextWindowBodyInstallRenderContext<'a>) -> Self {
        Self { context }
    }

    pub(crate) fn install_and_apply(
        self,
        state: TextWindowBodyInstallState<'_, '_, '_>,
    ) -> TextWindowRedisplayPositions {
        let context = self.context;
        let right_edge_markers = TextWindowRightEdgeMarkers::for_reserved_special_column(
            context.reserve_right_special_col,
            context.reserve_right_border_col,
            context.display_text_row_base,
            context.output_cols,
            context.row_flags,
            context.right_edge_face_id,
            context.right_edge_face,
            context.char_w,
        );

        install_text_window_body_output(
            state.output,
            state.output_emitter,
            TextWindowBodyOutputInstall {
                window_id: context.window_id,
                window_start: context.window_start,
                text_start_byte: context.text_start_byte,
                byte_idx: context.byte_idx,
                right_edge_markers,
            },
            Some(state.render_services),
        )
    }
}

impl<'a, 'buf, B: LayoutBufferView> TextWindowVisibilityRetryRequest<'a, 'buf, B> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        rows: &'a [DisplayRowSnapshot],
        window_start: i64,
        accessible_start: i64,
        accessible_end: i64,
        point_charpos: i64,
        charpos: i64,
        point_is_visible_eob: bool,
        text_area_top: i64,
        text_area_bottom: i64,
        scroll_policy: ScrollPolicy,
        scroll_margin: i64,
        buf_access: &'a RustBufferAccess<'buf, B>,
    ) -> Self {
        Self {
            rows,
            window_start,
            accessible_start,
            accessible_end,
            point_charpos,
            charpos,
            point_is_visible_eob,
            text_area_top,
            text_area_bottom,
            scroll_policy,
            scroll_margin,
            buf_access,
        }
    }

    pub(crate) fn decide(self) -> TextWindowVisibilityRetryOutcome {
        let point_lisp = layout_i64_char_pos_to_lisp_char_pos(self.point_charpos);
        let visible_end_lisp = self.rows.iter().rev().find_map(|row| row.end_buffer_pos);
        let visible_end_lisp = if self.point_is_visible_eob {
            Some(visible_end_lisp.unwrap_or(point_lisp).max(point_lisp))
        } else {
            visible_end_lisp
        };
        let visible_progress = visible_end_lisp
            .map(LispCharPos1::as_i64)
            .unwrap_or(self.charpos);
        let point_beyond_visible_span = visible_end_lisp
            .map(|end_lisp| point_lisp > end_lisp)
            .unwrap_or(self.point_charpos > self.charpos);

        // GNU `resize_mini_window` (src/xdisp.c:13361) scrolls a mini-window
        // whose measured content exceeds `max-mini-window-height` so the END
        // (where point sits in an active fido/vertico minibuffer) shows.  This
        // is the same point-driven scroll an ordinary window uses, so the
        // minibuffer is no longer excluded.  An inactive echo-area mini-window
        // has point reset to BEGV, so `point_beyond_visible_span` is false and
        // it never scrolls.
        let scroll_down_window_start = if visible_progress > self.window_start {
            self.scroll_down_start(point_lisp, visible_end_lisp, point_beyond_visible_span)
        } else {
            None
        };
        let point_row_window_start = next_window_start_for_partially_visible_point_row(
            self.rows,
            self.point_charpos,
            self.text_area_top,
            self.text_area_bottom,
            self.window_start,
        );
        let point_line_window_start = next_window_start_for_point_line_continuation(
            self.rows,
            self.point_charpos,
            self.window_start,
            self.buf_access,
            self.accessible_end,
        );

        TextWindowVisibilityRetryOutcome {
            visible_end_lisp,
            visible_progress,
            point_beyond_visible_span,
            scroll_down_window_start,
            point_row_window_start,
            point_line_window_start,
        }
    }

    /// New window start when point laid out below the bottom scroll margin —
    /// GNU `try_scrolling` (src/xdisp.c:19487-19556) run against the rows we
    /// just measured, then its `recenter:` fallback (src/xdisp.c:21108).
    /// `None` when point is where it belongs and nothing should move.
    ///
    /// Scrolling to `visible_end` (a whole windowful) is what GNU never does:
    /// it is the "arbitrary page break" users see when walking down a buffer
    /// one line at a time.
    fn scroll_down_start(
        &self,
        point_lisp: LispCharPos1,
        visible_end_lisp: Option<LispCharPos1>,
        point_beyond_visible_span: bool,
    ) -> Option<i64> {
        let byte_at_charpos = |charpos: i64| {
            self.buf_access
                .byte_at(self.buf_access.charpos_to_bytepos(charpos))
        };
        // Rows already laid out below the start, and the lowest one point may
        // occupy once the bottom scroll-margin is honored (GNU's
        // `scroll_margin_y`, xdisp.c:19420).
        let laid_out_rows = visible_rows_below(self.rows, self.window_start);
        let bottom_row = last_usable_row(laid_out_rows as usize, self.scroll_margin);

        // A point below the measured rows can be extrapolated in source lines
        // only when source and display progress monotonically together.  If an
        // intervening property may consume source text, advance solely through
        // rows this render actually measured and retry from that proven
        // boundary.  The next render can then walk the fold/replacement and
        // either place point or make another measured step.
        let first_unmeasured = visible_end_lisp
            .map(crate::coords::lisp_char_pos_to_layout_i64)
            .unwrap_or(self.charpos);
        let forward_measurement = point_beyond_visible_span.then(|| {
            self.buf_access
                .forward_scroll_measurement(first_unmeasured, self.point_charpos)
        });
        if forward_measurement == Some(ForwardScrollMeasurement::DisplayRowsRequired) {
            return next_window_start_from_visible_rows(
                self.rows,
                self.window_start,
                laid_out_rows,
            );
        }

        // Which display row point landed on. Inside the laid-out rows this is
        // exact — it accounts for wrapped lines and for text the display hides.
        // Below them nothing was measured, so estimate one row per newline.
        let point_row_in_rows = self.rows.iter().position(|row| {
            row.start_buffer_pos
                .zip(row.end_buffer_pos)
                .is_some_and(|(start, end)| start <= point_lisp && point_lisp <= end)
        });
        let (point_row, bounded) = match point_row_in_rows {
            Some(row) => (row as i64, true),
            // Point is off the bottom: nothing measured it, so estimate one row
            // per newline below the visible end.
            None if point_beyond_visible_span => {
                let first_hidden = visible_end_lisp.map_or(self.point_charpos, |end| end.as_i64());
                let (extra_lines, bounded) = count_lines_bounded(
                    first_hidden,
                    self.point_charpos,
                    self.scroll_policy.search_limit_lines(),
                    &byte_at_charpos,
                );
                (laid_out_rows + extra_lines, bounded)
            }
            // Point is not on any row and not below them — it is ABOVE the
            // window, which this branch must never answer for.
            None => return None,
        };
        // GNU's `dy`. Zero or less means point is already where the margin
        // allows, and `try_scrolling` leaves the window alone.
        let dy = point_row - bottom_row;
        if dy <= 0 {
            return None;
        }

        let advance = |lines: i64| {
            let start = next_window_start_from_visible_rows(self.rows, self.window_start, lines)?;
            // Rows below the window were never laid out; walk the remainder in
            // buffer lines. Under-shooting is safe — the retry runs again.
            let unsatisfied = lines - laid_out_rows.min(lines);
            Some(line_start_below(
                start,
                unsatisfied,
                self.accessible_end,
                &byte_at_charpos,
            ))
        };

        match self.scroll_policy.forward_scroll(
            dy,
            bounded,
            laid_out_rows as usize,
            self.scroll_margin,
        ) {
            ForwardScroll::Advance { lines } => advance(lines),
            // `line_start_above` counts BUFFER lines, so on a single logical
            // line wrapped over many rows it walks back to the line start —
            // behind the current window. Recentering that way would leave point
            // off screen forever (the retry only accepts a start that advances),
            // so fall back to the measured display-row scroll.
            ForwardScroll::Recenter { lines_above_point } => Some(line_start_above(
                self.point_charpos,
                lines_above_point,
                self.accessible_start,
                &byte_at_charpos,
            ))
            .filter(|start| *start > self.window_start)
            .or_else(|| advance(dy)),
        }
    }
}

impl TextWindowFinishRequest {
    pub(crate) fn new(
        cell_origin: CellOrigin,
        regions: PresentedWindowRegions,
        mode_line_height: i64,
        header_line_height: i64,
        tab_line_height: i64,
    ) -> Self {
        Self {
            cell_origin,
            regions,
            mode_line_height,
            header_line_height,
            tab_line_height,
        }
    }

    pub(crate) fn finish_and_snapshot(
        self,
        state: TextWindowFinishState<'_>,
    ) -> TextWindowFinishOutput {
        let (_hit_rows, snapshot) = state.finish_snapshot(
            self.cell_origin,
            self.regions,
            self.mode_line_height,
            self.header_line_height,
            self.tab_line_height,
        );
        TextWindowFinishOutput { snapshot }
    }
}

impl TextWindowVisibilityRetryOutcome {
    pub(crate) fn visible_end_lisp(self) -> Option<LispCharPos1> {
        self.visible_end_lisp
    }

    #[cfg(test)]
    pub(crate) fn point_beyond_visible_span(self) -> bool {
        self.point_beyond_visible_span
    }

    pub(crate) fn scroll_down_window_start(self) -> Option<i64> {
        self.scroll_down_window_start
    }

    pub(crate) fn point_row_window_start(self) -> Option<i64> {
        self.point_row_window_start
    }

    pub(crate) fn point_line_window_start(self) -> Option<i64> {
        self.point_line_window_start
    }

    pub(crate) fn retry_window_start(self) -> Option<i64> {
        self.scroll_down_window_start
            .or(self.point_row_window_start)
            .or(self.point_line_window_start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_window_append_surface_request_reserves_right_columns() {
        let tab_stops = vec![4, 12];
        let surface =
            TextWindowAppendSurfaceRequest::new(20.0, 200.0, 16.0, true, true, 8.0, 6, &tab_stops)
                .into_surface();

        assert_eq!(surface.content_x(), 20.0);
        // GNU xdisp reserves both the continuation glyph and a non-rightmost
        // terminal window's border before it consumes body text.  The marker
        // must not replace the final source character at its display column.
        assert_eq!(surface.right_edge(), 188.0);
        assert_eq!(surface.full_text_right_edge(), 204.0);
    }
}
