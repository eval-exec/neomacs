//! Buffer source post-loop tail rendering and install context.

use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::display_cursor::CursorCaptureState;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::{
    DisplayRowFlags, DisplayRowGeometryState, DisplayRowLimit, DisplayRowYPositions,
};
use crate::display_row::overlay_string::BufferOverlayStringTextRowRenderContext;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::walk_state::{
    DisplayRowSourceStart, FaceScanCheckpoint, LineNumberRenderState,
};
use crate::display_source_progress::DisplaySourceRowProgressState;
use crate::display_text_window_row_lifecycle::{
    TextWindowBodyInstallRenderContext, TextWindowBodyInstallRequest,
    TextWindowCursorPublishStatus, TextWindowFinishRequest, TextWindowFinishState,
    TextWindowTailFinalizeContext, TextWindowTailFinalizeRequest, TextWindowTailFinalizeState,
    TextWindowVisibilityRetryOutcome, TextWindowVisibilityRetryRequest,
};
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{LayoutBufferView, ResolvedFace, RustBufferAccess};
use crate::scroll_policy::ScrollPolicy;
use crate::types::WindowParams;
use crate::viewport_resolution::ForwardViewportMeasurement;
use crate::window_layout::WindowChromeMetrics;
use neomacs_display_protocol::types::FaceId;
use neovm_core::window::{
    DisplayRowSnapshot, PresentedWindowRegions, WindowPresentationSnapshot, geometry::CellOrigin,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceBodyInstallContext {
    output_window_id: u64,
    display_text_row_base: usize,
    output_cols: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceRetryBounds {
    text_area_top: i64,
    text_area_bottom: i64,
}

pub(crate) struct BufferSourceTailRequestContext<'a> {
    pub(crate) params: &'a WindowParams,
    pub(crate) window_start: i64,
    accessible_start: i64,
    accessible_end: i64,
    text_start_byte: usize,
    display_text_row_base: usize,
    text_area_left: f32,
    window_top: f32,
    text_y: f32,
    text_height: f32,
    char_width: f32,
    char_height: f32,
    row_limit: DisplayRowLimit,
    retry_bounds: BufferSourceRetryBounds,
    forward_viewport_measurement: Option<ForwardViewportMeasurement>,
    body_install_context: BufferSourceBodyInstallContext,
    reserve_right_special_col: bool,
    reserve_right_border_col: bool,
    right_edge_face_id: FaceId,
    right_edge_face: &'a ResolvedFace,
    cell_origin: CellOrigin,
    regions: PresentedWindowRegions,
}

/// Cursor-row proof from the completed real source/overlay producer. This
/// numeric result belongs to one exclusive attempt and carries no Lisp owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceEobCursorRow {
    Unchanged,
    Unavailable { boundary: GnuMiniEobSourceBoundary },
}

/// Actual buffer-row boundary before EOB strings. The source producer owns
/// this numeric fact exclusively; it carries no Lisp state between mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GnuMiniEobSourceBoundary {
    BufferScreenLine,
    FreshLineAfterBufferNewline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourcePostLoopRenderOutcome {
    pub(crate) retry: TextWindowVisibilityRetryOutcome,
    pub(crate) rendered_rows_len: usize,
    pub(crate) cursor_publish_status: TextWindowCursorPublishStatus,
    pub(crate) eob_cursor_row: BufferSourceEobCursorRow,
}

impl BufferSourceBodyInstallContext {
    pub(crate) fn new(
        output_window_id: u64,
        display_text_row_base: usize,
        output_cols: usize,
    ) -> Self {
        Self {
            output_window_id,
            display_text_row_base,
            output_cols,
        }
    }

    pub(crate) fn display_text_row_base(self) -> usize {
        self.display_text_row_base
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request<'a>(
        self,
        window_start: i64,
        text_start_byte: usize,
        byte_idx: usize,
        reserve_right_special_col: bool,
        reserve_right_border_col: bool,
        row_flags: &'a DisplayRowFlags,
        right_edge_face_id: FaceId,
        right_edge_face: &'a ResolvedFace,
        char_width: f32,
    ) -> TextWindowBodyInstallRequest<'a> {
        TextWindowBodyInstallRequest::new(TextWindowBodyInstallRenderContext::new(
            self.output_window_id,
            window_start,
            text_start_byte,
            byte_idx,
            reserve_right_special_col,
            reserve_right_border_col,
            self.display_text_row_base,
            self.output_cols,
            row_flags,
            right_edge_face_id,
            right_edge_face,
            char_width,
        ))
    }

    #[cfg(test)]
    pub(crate) fn output_cols(self) -> usize {
        self.output_cols
    }
}

impl BufferSourceRetryBounds {
    pub(crate) fn new(text_area_top: i64, text_area_bottom: i64) -> Self {
        Self {
            text_area_top,
            text_area_bottom,
        }
    }

    pub(crate) fn text_area_top(self) -> i64 {
        self.text_area_top
    }

    pub(crate) fn text_area_bottom(self) -> i64 {
        self.text_area_bottom
    }
}

impl<'a> BufferSourceTailRequestContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        params: &'a WindowParams,
        window_start: i64,
        accessible_start: i64,
        accessible_end: i64,
        text_start_byte: usize,
        display_text_row_base: usize,
        text_area_left: f32,
        window_top: f32,
        text_y: f32,
        text_height: f32,
        char_width: f32,
        char_height: f32,
        row_limit: DisplayRowLimit,
        retry_bounds: BufferSourceRetryBounds,
        forward_viewport_measurement: Option<ForwardViewportMeasurement>,
        body_install_context: BufferSourceBodyInstallContext,
        reserve_right_special_col: bool,
        reserve_right_border_col: bool,
        right_edge_face_id: FaceId,
        right_edge_face: &'a ResolvedFace,
        cell_origin: CellOrigin,
        regions: PresentedWindowRegions,
    ) -> Self {
        Self {
            params,
            window_start,
            accessible_start,
            accessible_end,
            text_start_byte,
            display_text_row_base,
            text_area_left,
            window_top,
            text_y,
            text_height,
            char_width,
            char_height,
            row_limit,
            retry_bounds,
            forward_viewport_measurement,
            body_install_context,
            reserve_right_special_col,
            reserve_right_border_col,
            right_edge_face_id,
            right_edge_face,
            cell_origin,
            regions,
        }
    }

    pub(crate) fn tail_finalize_request<'request>(
        &'request self,
        text: &'request [u8],
        charpos: i64,
        point_is_visible_eob: bool,
    ) -> TextWindowTailFinalizeRequest<'request> {
        TextWindowTailFinalizeRequest::new(TextWindowTailFinalizeContext::new(
            self.params,
            text,
            self.display_text_row_base,
            self.text_area_left,
            self.window_top,
            self.text_y,
            self.text_height,
            self.char_width,
            self.char_height,
            self.window_start,
            self.params.point_charpos().get(),
            charpos,
            point_is_visible_eob,
            self.row_limit,
        ))
    }

    pub(crate) fn visibility_retry_request<'rows, 'buf, B>(
        &'rows self,
        rows: &'rows [DisplayRowSnapshot],
        charpos: i64,
        point_is_visible_eob: bool,
        buf_access: &'rows RustBufferAccess<'buf, B>,
    ) -> TextWindowVisibilityRetryRequest<'rows, 'buf, B>
    where
        B: LayoutBufferView,
    {
        TextWindowVisibilityRetryRequest::new(
            rows,
            self.window_start,
            self.accessible_start,
            self.accessible_end,
            self.params.point_charpos().get(),
            charpos,
            point_is_visible_eob,
            self.retry_bounds.text_area_top(),
            self.retry_bounds.text_area_bottom(),
            ScrollPolicy::from_window_params(self.params),
            self.params.scroll_margin,
            self.forward_viewport_measurement.as_ref(),
            buf_access,
        )
    }

    pub(crate) fn body_install_request<'request>(
        &'request self,
        byte_idx: usize,
        row_flags: &'request DisplayRowFlags,
    ) -> TextWindowBodyInstallRequest<'request> {
        self.body_install_context.request(
            self.window_start,
            self.text_start_byte,
            byte_idx,
            self.reserve_right_special_col,
            self.reserve_right_border_col,
            row_flags,
            self.right_edge_face_id,
            self.right_edge_face,
            self.char_width,
        )
    }

    pub(crate) fn finish_request(
        &self,
        measured_chrome_heights: WindowChromeMetrics,
    ) -> TextWindowFinishRequest {
        // Report the chrome rows' *measured* heights (GNU `w->mode_line_height`)
        // — a tall `display` element grows these past the face-only estimate the
        // text-area geometry was reserved from.
        TextWindowFinishRequest::new(
            self.cell_origin,
            self.regions,
            measured_chrome_heights.mode_line_height as i64,
            measured_chrome_heights.header_line_height as i64,
            measured_chrome_heights.tab_line_height as i64,
        )
    }

    pub(crate) fn finish_and_install(
        &self,
        finish_state: TextWindowFinishState<'_>,
        measured_chrome_heights: WindowChromeMetrics,
        window_snapshots: &mut Vec<WindowPresentationSnapshot>,
    ) {
        let finished_window = self
            .finish_request(measured_chrome_heights)
            .finish_and_snapshot(finish_state);
        window_snapshots.push(WindowPresentationSnapshot::live(
            finished_window.into_snapshot(),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_buffer_source_tail_and_decide_retry<
    'request,
    'rows,
    'emit,
    'surface,
    'buf,
    B: LayoutBufferView,
>(
    loop_context: BufferSourceLoopRequestContext,
    mut source_render: TextRowSourceRenderState<'emit>,
    mut row_progress: DisplaySourceRowProgressState<'emit>,
    row_geometry: &'emit mut DisplayRowGeometryState,
    cursor_info: &'emit mut CursorCaptureState,
    row_source_start: &'emit mut DisplayRowSourceStart,
    row_y_positions: &'rows mut DisplayRowYPositions,
    face_ids: &'emit mut FrameFaceAttempt,
    line_numbers: &'emit mut LineNumberRenderState,
    face_scan: &'emit mut FaceScanCheckpoint,
    overlay_context: BufferOverlayStringTextRowRenderContext<'surface>,
    tail_context: &BufferSourceTailRequestContext<'_>,
    text: &'request [u8],
    byte_idx: usize,
    charpos: i64,
    active_face_state: &'request DisplayRowActiveFaceState,
    buffer: &B,
    buf_access: &'rows RustBufferAccess<'buf, B>,
    source_stop: crate::buffer_source::loop_render::BufferSourceVisibleLoopOutcome,
) -> BufferSourcePostLoopRenderOutcome
where
    'surface: 'request,
{
    if source_stop
        == crate::buffer_source::loop_render::BufferSourceVisibleLoopOutcome::QueryTargetReached
    {
        // GNU move_it_to stops at the stable target iterator. Complete the
        // observed row's metrics, without EOB anchors or after-strings at the
        // unconsumed position. The final row remains a certified prefix.
        let finalized = tail_context
            .tail_finalize_request(text, charpos, false)
            .finalize_and_apply(TextWindowTailFinalizeState::new(
                cursor_info,
                row_geometry,
                row_y_positions,
                row_source_start,
                source_render.output_render(),
            ));
        let retry = tail_context
            .visibility_retry_request(source_render.output_rows(), charpos, false, buf_access)
            .decide();
        return BufferSourcePostLoopRenderOutcome {
            retry,
            rendered_rows_len: source_render.output_rows_len(),
            cursor_publish_status: finalized.cursor_publish_status(),
            eob_cursor_row: BufferSourceEobCursorRow::Unchanged,
        };
    }
    let boundary = if text.last() == Some(&b'\n')
        && row_source_start.covers(charpos)
        && !row_source_start.covers(charpos.saturating_sub(1))
        && source_render.output_rows().last().is_some_and(|row| {
            row.end_source == neovm_core::window::DisplayRowEndSource::Buffer
                && row.end_buffer_pos == Some(neovm_core::buffer::LispCharPos1::new(charpos))
        }) {
        GnuMiniEobSourceBoundary::FreshLineAfterBufferNewline
    } else {
        GnuMiniEobSourceBoundary::BufferScreenLine
    };
    let tail_progress = loop_context
        .end_of_buffer_tail_request(byte_idx, charpos, overlay_context, active_face_state)
        .with_overlay_policy(match source_stop {
            crate::buffer_source::loop_render::BufferSourceVisibleLoopOutcome::MiniSourcePositionReached =>
                crate::buffer_source::row_lifecycle::BufferSourceEobOverlayPolicy::SourcePositionReached,
            _ => crate::buffer_source::row_lifecycle::BufferSourceEobOverlayPolicy::Render,
        })
        .render_and_apply(
            buffer,
            source_render.reborrow(),
            row_progress.reborrow(),
            row_geometry,
            cursor_info,
            row_source_start,
            row_y_positions,
            face_ids,
            line_numbers,
            face_scan,
        );
    let point_is_visible_eob = tail_progress.point_is_visible_eob();

    let finalized = tail_context
        .tail_finalize_request(text, charpos, point_is_visible_eob)
        .finalize_and_apply(TextWindowTailFinalizeState::new(
            cursor_info,
            row_geometry,
            row_y_positions,
            row_source_start,
            source_render.output_render(),
        ));

    let eob_cursor_row = if tail_context.params.is_minibuffer()
        && !tail_context.params.window_system
        && match tail_progress.progress {
            crate::buffer_source::row_lifecycle::BufferSourceEobTailProgress::OverlayRowsExhausted => true,
            crate::buffer_source::row_lifecycle::BufferSourceEobTailProgress::SourceRowsExhausted =>
                boundary == GnuMiniEobSourceBoundary::FreshLineAfterBufferNewline,
            crate::buffer_source::row_lifecycle::BufferSourceEobTailProgress::Complete => false,
        }
        && source_render.output_render().with_output_target_parts(|_, _, eval| {
            eval.gnu_redisplay_hooks_policy_enabled() && eval.gnu_redisplay_transaction_active()
        })
    {
        // GNU row_for_charpos_p admits a continued string row or an explicit
        // cursor property, but an ordinary overlay newline cannot own point.
        // Inspect only rows the actual producer completed inside the text box.
        let point = crate::coords::layout_i64_char_pos_to_lisp_char_pos(tail_context.params.point_charpos().get());
        let admits_cursor = cursor_info.eob_row_candidates().any(|(offset, explicit)| {
            let index = tail_context.display_text_row_base.saturating_add(offset);
            let Some(row) = source_render.output_rows().iter().find(|row| row.row == index as i64) else {
                return false;
            };
            let top = row.y;
            let bottom = top.saturating_add(row.height);
            let fully_visible = top >= tail_context.retry_bounds.text_area_top()
                && bottom <= tail_context.retry_bounds.text_area_bottom();
            // GNU cursor_row_fully_visible_p accepts a mini cursor row at
            // least as tall as its window, provided that row is on screen.
            let visible_oversized = row.height >= tail_context.params.bounds.height.round() as i64
                && top < tail_context.retry_bounds.text_area_bottom()
                && bottom > tail_context.retry_bounds.text_area_top();
            if !fully_visible && !visible_oversized {
                return false;
            }
            let ordinary_overlay_newline = row.end_buffer_pos == Some(point)
                && row.end_source == neovm_core::window::DisplayRowEndSource::OverlayAfterString;
            explicit == crate::display_cursor::EobCursorCandidateKind::ExplicitStringProperty || !ordinary_overlay_newline
                || source_render.output_render().with_output_target_parts(|mut output, _, _| {
                    output.builder().current_window_row(index).is_some_and(|row| row.continued)
                })
        });
        if admits_cursor { BufferSourceEobCursorRow::Unchanged } else { BufferSourceEobCursorRow::Unavailable { boundary } }
    } else {
        BufferSourceEobCursorRow::Unchanged
    };

    // GNU redisplay keeps iterating until point visibility converges or no
    // further progress can be made. Advance by actual rendered row spans
    // from this pass, since wrapped and variable-height lines are exactly
    // where newline-based retry selection goes wrong.
    let retry = tail_context
        .visibility_retry_request(
            source_render.output_rows(),
            charpos,
            point_is_visible_eob,
            buf_access,
        )
        .decide();
    BufferSourcePostLoopRenderOutcome {
        retry,
        rendered_rows_len: source_render.output_rows_len(),
        cursor_publish_status: finalized.cursor_publish_status(),
        eob_cursor_row,
    }
}
