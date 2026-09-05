//! Buffer source body walk setup and render pass driver.

use crate::buffer_source::face_resolution::*;
use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::buffer_source::loop_state::{
    BufferSourceLoopMutableState, BufferSourceRowBuildState, BufferSourceRowCarryoverState,
    BufferSourceSurfaceContext,
};
use crate::buffer_source::render_attempt::{
    BufferSourceOutputState, BufferSourceRedisplayPublishRequest,
};
use crate::buffer_source::render_plan::BufferSourceDefaultFacePlan;
use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
use crate::buffer_source::tail_render::{
    BufferSourcePostLoopRenderOutcome, BufferSourceTailRequestContext,
    render_buffer_source_tail_and_decide_retry,
};
use crate::buffer_source::walk::BufferSourceWalk;
use crate::buffer_source::window_geometry::{BufferWindowGeometry, BufferWindowLocalDisplayPolicy};
use crate::buffer_source::window_source::BufferWindowSource;
use crate::display_cursor::CursorCaptureState;
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::builder::DisplayPhysicalLineTabState;
use crate::display_row::face_environment::FrameFaces;
use crate::display_row::face_state::{DisplayRowActiveFaceState, DisplayRowMeasurementMode};
use crate::display_row::geometry::{
    DisplayRowExtendState, DisplayRowFlags, DisplayRowGeometryDefaults, DisplayRowGeometryState,
    DisplayRowYPositions,
};
use crate::display_row::lisp_string::DisplayRowPrefixRequest;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::overlay_string::BufferOverlayStringTextRowRenderContext;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::walk_state::{
    BoxFaceRowState, DisplayRowSourceStart, FaceScanCheckpoint, HorizontalScrollSkipState,
    HorizontalScrollTruncationTarget, InvisibleTextScanCheckpoint, LineNumberRenderState,
    TrailingWhitespaceRenderState, WordWrapRenderState,
};
use crate::display_source_progress::{DisplaySourceProgressState, DisplaySourceRowProgressState};
use crate::display_status_line::ChromeRowRenderServices;
use crate::display_text_window_row_lifecycle::{
    TextWindowAppendSurfaceRequest, TextWindowBeginRequest, TextWindowBodyInstallState,
};
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{FaceResolver, LayoutBufferView, RustBufferAccess};
use crate::types::{LineWrapMode, WindowParams};
use crate::window_output::{
    TextWindowOutputTarget, TextWindowRedisplayPositions, WindowOutputEmitter,
};
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;

pub(crate) struct BufferSourceWalkSetupRequest<'a> {
    window_start: i64,
    content_x: f32,
    text_x: f32,
    text_width: f32,
    text_y: f32,
    window_top: f32,
    line_number_pixel_width: f32,
    max_rows: usize,
    metrics: DisplayRowFallbackMetrics,
    measurement_mode: DisplayRowMeasurementMode,
    wrap_mode: LineWrapMode,
    hscroll: i32,
    word_wrap: bool,
    has_prefix: bool,
    has_line_default_prefix: bool,
    reserve_right_border_col: bool,
    reserve_right_special_col: bool,
    tab_width: i32,
    tab_stop_list: &'a [i32],
    trailing_whitespace_enabled: bool,
    trailing_whitespace_bg: u32,
    image_scale_environment: ImageScaleEnvironment,
    left_margin_columns: usize,
    left_margin_width: f32,
    right_margin_columns: usize,
    right_margin_width: f32,
}

pub(crate) struct BufferSourceWalkSetup {
    pub(crate) x: f32,
    pub(crate) col: usize,
    pub(crate) byte_idx: usize,
    pub(crate) charpos: i64,
    pub(crate) text_area_left: f32,
    pub(crate) window_top: f32,
    pub(crate) invisible_text_checkpoint: InvisibleTextScanCheckpoint,
    pub(crate) row_flags: DisplayRowFlags,
    pub(crate) hscroll_skip: HorizontalScrollSkipState,
    pub(crate) word_wrap: WordWrapRenderState,
    pub(crate) physical_line_tabs: DisplayPhysicalLineTabState,
    pub(crate) prefix_request: DisplayRowPrefixRequest,
    pub(crate) text_append_surface: DisplayRowAppendSurface,
    pub(crate) row_geometry_defaults: DisplayRowGeometryDefaults,
    pub(crate) row_geometry: DisplayRowGeometryState,
    pub(crate) row_y_positions: DisplayRowYPositions,
    pub(crate) trailing_whitespace: TrailingWhitespaceRenderState,
    pub(crate) row_extend: DisplayRowExtendState,
    pub(crate) box_face: BoxFaceRowState,
    pub(crate) cursor_info: CursorCaptureState,
    pub(crate) row_source_start: DisplayRowSourceStart,
    pub(crate) beyond_accessible_end_line_prefix:
        Option<crate::buffer_source::end_of_buffer_rows::BeyondAccessibleEndLinePrefix>,
}

struct BufferSourceWalkRenderState<'emit> {
    source_render: TextRowSourceRenderState<'emit>,
    line_numbers: &'emit mut LineNumberRenderState,
    face_scan: &'emit mut FaceScanCheckpoint,
    active_face_state: &'emit mut DisplayRowActiveFaceState,
    face_ids: &'emit mut FrameFaceAttempt,
}

impl<'emit> BufferSourceWalkRenderState<'emit> {
    fn new(
        source_render: TextRowSourceRenderState<'emit>,
        line_numbers: &'emit mut LineNumberRenderState,
        face_scan: &'emit mut FaceScanCheckpoint,
        active_face_state: &'emit mut DisplayRowActiveFaceState,
        face_ids: &'emit mut FrameFaceAttempt,
    ) -> Self {
        Self {
            source_render,
            line_numbers,
            face_scan,
            active_face_state,
            face_ids,
        }
    }
}

impl<'a> BufferSourceWalkSetupRequest<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        window_start: i64,
        content_x: f32,
        text_x: f32,
        text_width: f32,
        text_y: f32,
        window_top: f32,
        line_number_pixel_width: f32,
        max_rows: usize,
        metrics: DisplayRowFallbackMetrics,
        measurement_mode: DisplayRowMeasurementMode,
        wrap_mode: LineWrapMode,
        hscroll: i32,
        word_wrap: bool,
        has_prefix: bool,
        has_line_default_prefix: bool,
        reserve_right_border_col: bool,
        reserve_right_special_col: bool,
        tab_width: i32,
        tab_stop_list: &'a [i32],
        trailing_whitespace_enabled: bool,
        trailing_whitespace_bg: u32,
    ) -> Self {
        Self {
            window_start,
            content_x,
            text_x,
            text_width,
            text_y,
            window_top,
            line_number_pixel_width,
            max_rows,
            metrics,
            measurement_mode,
            wrap_mode,
            hscroll,
            word_wrap,
            has_prefix,
            has_line_default_prefix,
            reserve_right_border_col,
            reserve_right_special_col,
            tab_width,
            tab_stop_list,
            trailing_whitespace_enabled,
            trailing_whitespace_bg,
            image_scale_environment: ImageScaleEnvironment::default(),
            left_margin_columns: 0,
            left_margin_width: 0.0,
            right_margin_columns: 0,
            right_margin_width: 0.0,
        }
    }

    fn with_margin_areas(
        mut self,
        left_columns: usize,
        left_width: f32,
        right_columns: usize,
        right_width: f32,
    ) -> Self {
        self.left_margin_columns = left_columns;
        self.left_margin_width = left_width;
        self.right_margin_columns = right_columns;
        self.right_margin_width = right_width;
        self
    }

    fn with_image_scale_environment(
        mut self,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        self.image_scale_environment = image_scale_environment;
        self
    }

    pub(crate) fn from_window_geometry(
        source: BufferWindowSource,
        params: &'a WindowParams,
        geometry: &BufferWindowGeometry,
        local_display_policy: &BufferWindowLocalDisplayPolicy,
        default_face: &BufferSourceDefaultFacePlan,
        reserve_right_border_col: bool,
        reserve_right_special_col: bool,
    ) -> Self {
        Self::new(
            source.window_start(),
            geometry.content_x,
            geometry.text_x,
            geometry.text_width,
            // Body row-walk origin. An ordinary window with an active vscroll walks
            // from `text_y - vscroll` (content scrolled up, GNU semantics); equals
            // `text_y` otherwise. The visible/clip band stays `text_y ..
            // text_y + text_height`.
            geometry.row_origin_y(),
            params.bounds.y,
            geometry.line_number_pixel_width,
            geometry.max_rows,
            default_face.row_metrics_for_body_width(geometry.char_width),
            default_face.measurement_policy().mode(),
            params.wrap_mode,
            params.hscroll,
            params.word_wrap,
            local_display_policy.has_prefix(),
            local_display_policy.has_line_default_prefix(),
            reserve_right_border_col,
            reserve_right_special_col,
            params.tab_width,
            &params.tab_stop_list,
            params.show_trailing_whitespace,
            params.trailing_ws_bg,
        )
        .with_margin_areas(
            params.left_margin_columns.max(0) as usize,
            params.left_margin_width,
            params.right_margin_columns.max(0) as usize,
            params.right_margin_width,
        )
        .with_image_scale_environment(params.image_scale_environment)
    }

    pub(crate) fn into_setup(self) -> BufferSourceWalkSetup {
        let row_geometry_defaults = DisplayRowGeometryDefaults::new(
            self.text_y,
            self.metrics.row_height(),
            self.metrics.ascent(),
            self.measurement_mode,
        );

        BufferSourceWalkSetup {
            x: self.content_x,
            col: 0,
            byte_idx: 0,
            charpos: self.window_start,
            text_area_left: self.text_x,
            window_top: self.window_top,
            invisible_text_checkpoint: InvisibleTextScanCheckpoint::new(self.window_start),
            row_flags: DisplayRowFlags::new(self.max_rows),
            hscroll_skip: HorizontalScrollSkipState::new(
                self.wrap_mode,
                self.hscroll,
                if self.line_number_pixel_width > 0.0 {
                    HorizontalScrollTruncationTarget::LineNumberPrefix
                } else {
                    HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph
                },
            ),
            word_wrap: WordWrapRenderState::new(self.word_wrap),
            physical_line_tabs: DisplayPhysicalLineTabState::default(),
            prefix_request: DisplayRowPrefixRequest::initial(
                self.has_prefix,
                self.has_line_default_prefix,
            ),
            text_append_surface: TextWindowAppendSurfaceRequest::new(
                self.content_x,
                self.text_width,
                self.line_number_pixel_width,
                self.reserve_right_border_col,
                self.reserve_right_special_col,
                self.metrics.char_width(),
                self.tab_width,
                self.tab_stop_list,
            )
            .into_surface()
            .with_margin_areas(
                self.left_margin_columns,
                self.left_margin_width,
                self.right_margin_columns,
                self.right_margin_width,
            )
            .with_image_scale_environment(self.image_scale_environment),
            row_geometry_defaults,
            row_geometry: row_geometry_defaults.initial_state(),
            row_y_positions: DisplayRowYPositions::with_capacity_and_first_row(
                self.max_rows,
                self.text_y,
            ),
            trailing_whitespace: TrailingWhitespaceRenderState::new(
                self.trailing_whitespace_enabled,
                self.trailing_whitespace_bg,
            ),
            row_extend: DisplayRowExtendState::inactive(),
            box_face: BoxFaceRowState::inactive(),
            cursor_info: CursorCaptureState::new(),
            row_source_start: DisplayRowSourceStart::new(self.window_start),
            beyond_accessible_end_line_prefix: None,
        }
    }
}

impl BufferSourceWalkSetup {
    #[allow(clippy::too_many_arguments)]
    fn render_visible_steps<'request, B: LayoutBufferView>(
        &mut self,
        state: &mut BufferSourceWalkRenderState<'_>,
        row_prelude_context: BufferSourceRowPreludeRequestContext,
        loop_context: BufferSourceLoopRequestContext,
        face_resolution_context: BufferSourceFaceResolutionContext<'request, B>,
        text: &'request [u8],
        params: &WindowParams,
        overlay_text_row_context: BufferOverlayStringTextRowRenderContext<'request>,
        buffer: &B,
    ) {
        let mut source_walk = BufferSourceWalk::new_for_window(
            loop_context.buffer_id(),
            buffer,
            Some(params.window_id as u64),
            self.charpos,
            loop_context.text_start_byte(),
        );

        BufferSourceLoopMutableState::new(
            &mut self.invisible_text_checkpoint,
            DisplaySourceProgressState::new(
                &mut self.byte_idx,
                &mut self.charpos,
                &mut self.x,
                &mut self.col,
            )
            .with_physical_line_tabs(&mut self.physical_line_tabs),
            state.source_render.reborrow(),
            BufferSourceRowBuildState::new(
                &mut self.row_geometry,
                &mut self.row_flags,
                &mut self.row_extend,
                &mut self.box_face,
            ),
            &mut self.row_source_start,
            BufferSourceRowCarryoverState::new(
                &mut self.prefix_request,
                state.line_numbers,
                &mut self.hscroll_skip,
                &mut self.word_wrap,
                &mut self.trailing_whitespace,
            ),
            state.face_scan,
            &mut self.row_y_positions,
            &mut self.cursor_info,
            state.face_ids,
            BufferSourceSurfaceContext::new(&self.text_append_surface, overlay_text_row_context),
        )
        .with_beyond_accessible_end_line_prefix_capture(&mut self.beyond_accessible_end_line_prefix)
        .render_visible_steps(
            loop_context,
            &mut source_walk,
            row_prelude_context,
            face_resolution_context,
            text,
            params,
            state.active_face_state,
            buffer,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn render_tail_and_decide_retry<'request, 'buf, B: LayoutBufferView>(
        &mut self,
        source_render: TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        line_numbers: &mut LineNumberRenderState,
        face_scan: &mut FaceScanCheckpoint,
        loop_context: BufferSourceLoopRequestContext,
        tail_context: &BufferSourceTailRequestContext<'_>,
        text: &'request [u8],
        overlay_context: BufferOverlayStringTextRowRenderContext<'request>,
        active_face_state: &'request DisplayRowActiveFaceState,
        buffer: &B,
        buf_access: &RustBufferAccess<'buf, B>,
    ) -> BufferSourcePostLoopRenderOutcome {
        render_buffer_source_tail_and_decide_retry(
            loop_context,
            source_render,
            DisplaySourceRowProgressState::new(&mut self.x, &mut self.col),
            &mut self.row_geometry,
            &mut self.cursor_info,
            &mut self.row_source_start,
            &mut self.row_y_positions,
            face_ids,
            line_numbers,
            face_scan,
            overlay_context,
            tail_context,
            text,
            self.byte_idx,
            self.charpos,
            active_face_state,
            buffer,
            buf_access,
        )
    }

    pub(crate) fn install_body(
        &mut self,
        output: TextWindowOutputTarget<'_>,
        output_emitter: &mut WindowOutputEmitter,
        render_services: ChromeRowRenderServices<'_, '_>,
        tail_context: &BufferSourceTailRequestContext<'_>,
    ) -> TextWindowRedisplayPositions {
        tail_context
            .body_install_request(self.byte_idx, &self.row_flags)
            .install_and_apply(TextWindowBodyInstallState::new(
                output,
                output_emitter,
                render_services,
            ))
    }

    pub(crate) fn install_body_and_publish_redisplay(
        &mut self,
        output: TextWindowOutputTarget<'_>,
        output_emitter: &mut WindowOutputEmitter,
        evaluator: &mut Context,
        render_services: ChromeRowRenderServices<'_, '_>,
        tail_context: &BufferSourceTailRequestContext<'_>,
        publish_request: BufferSourceRedisplayPublishRequest,
    ) -> TextWindowRedisplayPositions {
        let redisplay_positions =
            self.install_body(output, output_emitter, render_services, tail_context);
        // GNU status-line percent specs read the live window state from the
        // just-produced redisplay. Publish before chrome rows are evaluated.
        publish_request.publish_window_end(evaluator, redisplay_positions);
        redisplay_positions
    }

    #[allow(clippy::too_many_arguments)]
    fn render_body_and_tail<'request, 'buf, B: LayoutBufferView>(
        &mut self,
        state: &mut BufferSourceWalkRenderState<'_>,
        row_prelude_context: BufferSourceRowPreludeRequestContext,
        loop_context: BufferSourceLoopRequestContext,
        face_resolution_context: BufferSourceFaceResolutionContext<'request, B>,
        tail_context: &BufferSourceTailRequestContext<'_>,
        text: &'request [u8],
        params: &'request WindowParams,
        overlay_text_row_context: BufferOverlayStringTextRowRenderContext<'request>,
        buffer: &B,
        buf_access: &RustBufferAccess<'buf, B>,
    ) -> BufferSourcePostLoopRenderOutcome {
        self.render_visible_steps(
            state,
            row_prelude_context,
            loop_context,
            face_resolution_context,
            text,
            params,
            overlay_text_row_context,
            buffer,
        );

        self.render_tail_and_decide_retry(
            state.source_render.reborrow(),
            state.face_ids,
            state.line_numbers,
            state.face_scan,
            loop_context,
            tail_context,
            text,
            overlay_text_row_context,
            state.active_face_state,
            buffer,
            buf_access,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_render_body_and_tail<'request, 'buf, B: LayoutBufferView>(
        &mut self,
        begin_request: TextWindowBeginRequest,
        output: &mut BufferSourceOutputState<'_>,
        font_metrics: &mut Option<FontMetricsService>,
        face_resolver: &FaceResolver,
        face_ids: &mut FrameFaceAttempt,
        line_numbers: &mut LineNumberRenderState,
        face_scan: &mut FaceScanCheckpoint,
        active_face_state: &mut DisplayRowActiveFaceState,
        row_prelude_context: BufferSourceRowPreludeRequestContext,
        loop_context: BufferSourceLoopRequestContext,
        face_resolution_context: BufferSourceFaceResolutionContext<'request, B>,
        tail_context: &BufferSourceTailRequestContext<'_>,
        text: &'request [u8],
        params: &'request WindowParams,
        overlay_text_row_context: BufferOverlayStringTextRowRenderContext<'request>,
        buffer: &B,
        buf_access: &RustBufferAccess<'buf, B>,
    ) -> (WindowOutputEmitter, BufferSourcePostLoopRenderOutcome) {
        let mut output_emitter = output.begin_text_window_output(begin_request);
        let source_render = output.source_render_state(
            &mut output_emitter,
            font_metrics,
            DisplayRowMeasurementMode::from_frame_window_system(params.window_system),
            FrameFaces::new(face_resolver).for_window(buffer),
        );
        let post_loop = self.render_body_and_tail(
            &mut BufferSourceWalkRenderState::new(
                source_render,
                line_numbers,
                face_scan,
                active_face_state,
                face_ids,
            ),
            row_prelude_context,
            loop_context,
            face_resolution_context,
            tail_context,
            text,
            params,
            overlay_text_row_context,
            buffer,
            buf_access,
        );
        (output_emitter, post_loop)
    }
}
