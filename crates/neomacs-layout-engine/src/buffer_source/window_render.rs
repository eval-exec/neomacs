//! Buffer window source rendering requests and actions.
use crate::buffer_source::body_render::BufferSourceWalkSetupRequest;
#[cfg(test)]
pub(crate) use crate::buffer_source::display_property_render::BufferDisplayPropertyTextReplacementOutcome;
use crate::buffer_source::render_attempt::WindowPositionPublication;
pub(crate) use crate::buffer_source::render_attempt::{
    BufferSourceRenderAttemptContext, BufferSourceRenderAttemptOutcome,
};
use crate::buffer_source::render_plan::{BufferSourceDefaultFacePlan, BufferSourceOutputSetup};
use crate::buffer_source::window_geometry::{
    BufferWindowGeometryPlan, BufferWindowGeometryRequest, BufferWindowLocalDisplayPolicy,
};
use crate::buffer_source::window_source::{BufferWindowSourceRequest, ResolvedWindowStart};
use crate::display_row::face_environment::FrameFaces;
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_status_line::{
    WindowChromeRowsPlan, max_mini_window_lines, max_mini_window_lines_for_buffer,
};
use crate::neovm_bridge::{LayoutBufferView, RustBufferAccess};
use crate::types::{FrameParams, WindowParams};
use crate::viewport_resolution::ForwardViewportMeasurement;
use crate::window_layout::WindowLayoutBox;
use neovm_core::buffer::BufferId;
use neovm_core::window::{FrameId, WindowId};

pub(crate) struct BufferWindowRenderRequest<'a, B>
where
    B: LayoutBufferView,
{
    frame_id: FrameId,
    window_id: WindowId,
    params: &'a WindowParams,
    frame_params: &'a FrameParams,
    layout_box: &'a WindowLayoutBox,
    buffer_id: BufferId,
    buffer: &'a B,
    buffer_name: &'a str,
    reserve_right_border_col: bool,
    position_publication: WindowPositionPublication,
    resolved_window_start: ResolvedWindowStart,
    forward_viewport_measurement: Option<ForwardViewportMeasurement>,
}

impl<'a, B> BufferWindowRenderRequest<'a, B>
where
    B: LayoutBufferView,
{
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        frame_id: FrameId,
        window_id: WindowId,
        params: &'a WindowParams,
        frame_params: &'a FrameParams,
        layout_box: &'a WindowLayoutBox,
        buffer_id: BufferId,
        buffer: &'a B,
        buffer_name: &'a str,
        reserve_right_border_col: bool,
        resolved_window_start: ResolvedWindowStart,
    ) -> Self {
        Self {
            frame_id,
            window_id,
            params,
            frame_params,
            layout_box,
            buffer_id,
            buffer,
            buffer_name,
            reserve_right_border_col,
            position_publication: WindowPositionPublication::Redisplay,
            resolved_window_start,
            forward_viewport_measurement: None,
        }
    }

    pub(crate) fn with_position_publication(
        mut self,
        publication: WindowPositionPublication,
    ) -> Self {
        self.position_publication = publication;
        self
    }

    pub(crate) fn with_forward_viewport_measurement(
        mut self,
        measurement: Option<ForwardViewportMeasurement>,
    ) -> Self {
        self.forward_viewport_measurement = measurement;
        self
    }

    pub(crate) fn render_into(
        self,
        context: BufferSourceRenderAttemptContext<'_, '_>,
        text_buf: &mut Vec<u8>,
        remaining_visibility_retries: usize,
        cursor_only: Option<crate::incremental_layout::CursorOnlyReplay>,
        scroll: Option<crate::incremental_layout::ScrollReplay>,
    ) -> BufferSourceRenderAttemptOutcome {
        let Self {
            frame_id,
            window_id,
            params,
            frame_params,
            layout_box,
            buffer_id,
            buffer,
            buffer_name,
            reserve_right_border_col,
            position_publication,
            resolved_window_start,
            forward_viewport_measurement,
        } = self;
        let mut state = context;
        let buf_access = RustBufferAccess::new(buffer);
        state.output_mut().install_cursor_effects(params);

        let char_w = params.char_width;
        let char_h = params.char_height;
        let window_metrics = DisplayRowFallbackMetrics::from_window_defaults(params);
        let local_display_policy = BufferWindowLocalDisplayPolicy::from_window(buffer, params);

        let default_face = state.with_face_services(|face_resolver, font_metrics| {
            BufferSourceDefaultFacePlan::new(
                face_resolver,
                buffer,
                font_metrics,
                DisplayRowMeasurementMode::from_frame_window_system(frame_params.window_system),
                window_metrics,
            )
        });
        let default_resolved = default_face.face();

        // GNU derives `lnum_pixel_width` from glyphs produced with the
        // window-resolved line-number face, not from FRAME_COLUMN_WIDTH.  Plan
        // the widest line-number role Neomacs can emit so every row keeps one
        // stable buffer-text origin even when face remapping changes font size.
        let line_number_cell_width = if params.display_line_numbers.enabled() {
            state.with_face_services(|face_resolver, font_metrics| {
                let faces = FrameFaces::new(face_resolver).for_window(buffer);
                crate::display_row::walk_state::LineNumberTextPrefixFace::ALL
                    .into_iter()
                    .map(|role| faces.resolve_named_face(role.face_name()))
                    .map(|face| {
                        font_metrics.as_mut().map_or(char_w, |service| {
                            service
                                .font_metrics(
                                    &face.font_family,
                                    face.font_weight,
                                    face.italic,
                                    face.font_size,
                                )
                                .char_width
                        })
                    })
                    .fold(char_w.max(1.0), f32::max)
            })
        } else {
            char_w
        };

        tracing::debug!(
            "layout font metrics: family={:?} weight={} italic={} size={} char_w={:.2} char_h={:.2} ascent={:.2} (window char_w={:.2} char_h={:.2})",
            default_resolved.font_family,
            default_resolved.font_weight,
            default_resolved.italic,
            default_resolved.font_size,
            default_face.char_width(),
            default_face.row_height(),
            default_face.ascent(),
            char_w,
            char_h,
        );

        let chrome_plan = state.with_face_services(|face_resolver, _font_metrics| {
            WindowChromeRowsPlan::new(params, buffer, face_resolver)
        });
        let max_mini_window_rows = {
            let frame_rows = frame_params.height / char_h.max(1.0);
            if params.is_minibuffer() {
                max_mini_window_lines_for_buffer(state.output_mut().evaluator(), buffer, frame_rows)
            } else {
                max_mini_window_lines(state.output_mut().evaluator(), frame_rows)
            }
            .ceil()
            .max(1.0) as usize
        };
        let BufferWindowGeometryPlan {
            mut geometry,
            line_number_field,
        } = BufferWindowGeometryRequest::new(params, layout_box, char_w, char_h)
            .with_max_mini_window_rows(max_mini_window_rows)
            .into_window_plan(&local_display_policy, &buf_access, line_number_cell_width);

        // Phase 2 pure-scroll: lay ONLY the newly-exposed rows. Start the body
        // walk at the exposed region (`text_y` + first row index); the unchanged
        // `visibility_bottom_y` caps it at the window bottom — exactly the
        // newly-exposed row count. Chrome geometry is left intact (the mode-line
        // re-walks at the full-window bottom). The source reads from
        // `walk_start` so its text slice + byte indices align with the
        // walk, while the published positions later use the real window_start.
        let source_request = if let Some(scroll) = &scroll {
            geometry.text_y = params.bounds.y + scroll.exposed_text_y;
            geometry.display_text_row_base = scroll.exposed_row_base;
            // Phase 3 below-reuse: BOUND the walk to the edited line only — the
            // rows below it are reused (charpos-shifted) and supplied as reused
            // rows. Without this the walk would run to the window bottom and
            // overwrite the reused-below rows (no CPU saving + wrong positions).
            if scroll.bound_walk {
                geometry.max_rows = scroll.exposed_row_count;
            }
            // Exact partial reads do not run the viewport scrolling heuristic,
            // so there is no need to counterfeit point.  The request keeps the
            // real semantic point for line numbers and other row decoration.
            BufferWindowSourceRequest::for_partial_walk(
                params,
                scroll.walk_start,
                geometry.max_rows,
            )
        } else {
            BufferWindowSourceRequest::from_window_params(params, geometry.max_rows)
        };

        let text_source = if scroll.is_some() {
            source_request.read_exact_into(&buf_access, text_buf)
        } else {
            source_request.read_resolved_into(resolved_window_start, &buf_access, text_buf)
        };
        let bytes_read = text_source.bytes_read();
        let text = if bytes_read > 0 {
            &text_buf[..bytes_read]
        } else {
            &[]
        };
        tracing::debug!(
            "  layout_window_rust id={}: text_y={:.1} text_h={:.1} max_rows={} bytes_read={}",
            params.window_id,
            geometry.text_y,
            geometry.text_height,
            geometry.max_rows,
            bytes_read
        );

        if geometry.text_height <= 0.0 || geometry.text_width <= 0.0 {
            return BufferSourceRenderAttemptOutcome::Skipped;
        }

        // A non-minibuffer window laid out at a degenerate (<= 1 row) height is a
        // transient/probe pass (child-frame/posframe or frame resize in flight).
        // Disable the visibility scroll retries for it: the retry would scroll
        // window_start to point — which then PERSISTS and corrupts the real (tall)
        // window. Pairs with the forward-scroll guard in
        // `BufferWindowSourceRequest::should_forward_scroll_without_layout`.
        let remaining_visibility_retries =
            if scroll.is_some() || (geometry.max_rows <= 1 && !params.is_minibuffer()) {
                // Phase 2 consumes the authoritative post-scroll window_start; never
                // re-derive scrolling via a visibility retry.
                0
            } else {
                remaining_visibility_retries
            };

        let reserve_right_special_col =
            !frame_params.window_system && params.right_fringe_width == 0.0;
        let mut walk_setup = BufferSourceWalkSetupRequest::from_window_geometry(
            text_source,
            params,
            &geometry,
            &local_display_policy,
            &default_face,
            reserve_right_border_col,
            reserve_right_special_col,
        )
        .into_setup();
        let text_append_surface = walk_setup.text_append_surface.clone();
        let output_setup = BufferSourceOutputSetup::from_window_geometry(
            frame_id,
            window_id,
            params,
            &geometry,
            layout_box,
            geometry.max_rows,
            &walk_setup,
        )
        .with_position_publication(position_publication);

        output_setup.render_body_attempt(
            &mut walk_setup,
            state,
            chrome_plan.render_request(
                params,
                *layout_box,
                geometry.mode_line_display_row,
                reserve_right_border_col,
                window_metrics,
                buffer_name,
            ),
            remaining_visibility_retries,
            forward_viewport_measurement,
            local_display_policy,
            line_number_field,
            &geometry,
            layout_box,
            buffer,
            buffer_id,
            text_source,
            params,
            &default_face,
            window_metrics,
            params.window_id as u64,
            &text_append_surface,
            reserve_right_special_col,
            reserve_right_border_col,
            text,
            &buf_access,
            cursor_only,
            scroll,
        )
    }
}
