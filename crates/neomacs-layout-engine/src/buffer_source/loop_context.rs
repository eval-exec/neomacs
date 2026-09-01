//! Buffer text loop request builders.

use crate::buffer_source::row_lifecycle::{
    BufferSourceEndOfBufferTailRenderContext, BufferSourceHscrollSkipRenderContext,
    BufferSourceInvisibleTextRenderContext, BufferSourceLineBreakRenderContext,
    BufferSourceLineBreakRenderRequest, BufferSourceSelectiveDisplayTailRenderContext,
    BufferSourceSelectiveDisplayTailRenderRequest,
};
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::{
    DisplayRowGeometryDefaults, DisplayRowLimit, DisplayRowVisibilityLimit,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::overlay_string::BufferOverlayStringTextRowRenderContext;
use crate::display_source::DisplaySourceStepChar;
use crate::types::WindowParams;
use neomacs_display_protocol::types::Color;
use neovm_core::buffer::BufferId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferSourceLoopRequestContext {
    buffer_id: BufferId,
    text_start_byte: usize,
    accessible_end: i64,
    point_charpos: i64,
    selective_display: i32,
    tab_width: i32,
    extra_line_spacing: f32,
    content_x: f32,
    has_prefix: bool,
    metrics: DisplayRowFallbackMetrics,
    row_visibility_limit: DisplayRowVisibilityLimit,
    row_geometry_defaults: DisplayRowGeometryDefaults,
    display_text_row_base: usize,
    max_rows: usize,
    row_limit: DisplayRowLimit,
    frame_background: Color,
    /// `display-fill-column-indicator` column (buffer columns), or negative when
    /// the indicator is disabled. GNU `fill_column_indicator_column`.
    fill_column_indicator: i32,
    fill_column_indicator_char: char,
}

impl BufferSourceLoopRequestContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        buffer_id: BufferId,
        text_start_byte: usize,
        accessible_end: i64,
        point_charpos: i64,
        params: &WindowParams,
        content_x: f32,
        has_prefix: bool,
        metrics: DisplayRowFallbackMetrics,
        row_visibility_limit: DisplayRowVisibilityLimit,
        row_geometry_defaults: DisplayRowGeometryDefaults,
        display_text_row_base: usize,
        max_rows: usize,
        row_limit: DisplayRowLimit,
        frame_background: Color,
    ) -> Self {
        Self {
            buffer_id,
            text_start_byte,
            accessible_end,
            point_charpos,
            selective_display: params.selective_display,
            tab_width: params.tab_width,
            extra_line_spacing: params.extra_line_spacing,
            content_x,
            has_prefix,
            metrics,
            row_visibility_limit,
            row_geometry_defaults,
            display_text_row_base,
            max_rows,
            row_limit,
            frame_background,
            fill_column_indicator: params.fill_column_indicator,
            fill_column_indicator_char: params.fill_column_indicator_char,
        }
    }

    pub(crate) fn invisible_text_request<'a>(
        self,
        text: &'a [u8],
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
    ) -> BufferSourceInvisibleTextRenderContext<'a> {
        BufferSourceInvisibleTextRenderContext::new(
            text,
            self.accessible_end,
            self.point_charpos,
            append_surface,
            active_face_state,
            glyph_y_offset,
            self.metrics,
        )
    }

    pub(crate) fn hscroll_skip_request<'a>(
        self,
        text: &'a [u8],
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
    ) -> BufferSourceHscrollSkipRenderContext<'a> {
        BufferSourceHscrollSkipRenderContext::new(
            text,
            self.tab_width,
            self.content_x,
            append_surface,
            active_face_state,
            self.metrics,
            self.point_charpos,
            self.has_prefix,
            self.row_geometry_defaults,
            self.display_text_row_base,
            self.max_rows,
            self.row_limit,
        )
    }

    pub(crate) fn selective_display_tail_request<'a>(
        self,
        source_step_char: DisplaySourceStepChar,
        text: &'a [u8],
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
    ) -> BufferSourceSelectiveDisplayTailRenderRequest<'a> {
        BufferSourceSelectiveDisplayTailRenderRequest::new(
            source_step_char,
            BufferSourceSelectiveDisplayTailRenderContext::new(
                text,
                self.text_start_byte,
                self.selective_display,
                self.tab_width,
                append_surface,
                active_face_state,
                glyph_y_offset,
                self.metrics,
                self.content_x,
                self.has_prefix,
                self.row_geometry_defaults,
                self.display_text_row_base,
                self.max_rows,
                self.row_limit,
            ),
        )
    }

    pub(crate) fn line_break_request<'a>(
        self,
        source_char: DisplaySourceStepChar,
        text: &'a [u8],
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
    ) -> BufferSourceLineBreakRenderRequest<'a> {
        BufferSourceLineBreakRenderRequest::new(
            source_char,
            BufferSourceLineBreakRenderContext::new(
                text,
                self.text_start_byte,
                self.selective_display,
                self.tab_width,
                active_face_state,
                self.point_charpos,
                self.metrics.row_height(),
                self.extra_line_spacing,
                self.content_x,
                self.has_prefix,
                self.row_geometry_defaults,
                self.display_text_row_base,
                self.max_rows,
                self.row_limit,
                append_surface,
                self.frame_background,
                self.fill_column_indicator,
                self.fill_column_indicator_char,
            ),
        )
    }

    pub(crate) fn end_of_buffer_tail_request<'a>(
        self,
        byte_idx: usize,
        charpos: i64,
        overlay_context: BufferOverlayStringTextRowRenderContext<'a>,
        active_face_state: &'a DisplayRowActiveFaceState,
    ) -> BufferSourceEndOfBufferTailRenderContext<'a> {
        BufferSourceEndOfBufferTailRenderContext::new(
            byte_idx,
            charpos,
            self.accessible_end,
            self.point_charpos,
            overlay_context,
            active_face_state,
        )
    }

    pub(crate) fn buffer_id(self) -> BufferId {
        self.buffer_id
    }

    pub(crate) fn text_start_byte(self) -> usize {
        self.text_start_byte
    }

    pub(crate) fn content_x(self) -> f32 {
        self.content_x
    }

    pub(crate) fn frame_background(self) -> Color {
        self.frame_background
    }

    pub(crate) fn char_height(self) -> f32 {
        self.metrics.row_height()
    }

    pub(crate) fn point_charpos(self) -> i64 {
        self.point_charpos
    }

    pub(crate) fn row_visibility_limit(self) -> DisplayRowVisibilityLimit {
        self.row_visibility_limit
    }

    pub(crate) fn has_prefix(self) -> bool {
        self.has_prefix
    }

    pub(crate) fn row_geometry_defaults(self) -> DisplayRowGeometryDefaults {
        self.row_geometry_defaults
    }

    pub(crate) fn display_text_row_base(self) -> usize {
        self.display_text_row_base
    }

    pub(crate) fn max_rows(self) -> usize {
        self.max_rows
    }

    pub(crate) fn row_limit(self) -> DisplayRowLimit {
        self.row_limit
    }

    pub(crate) fn selective_display(self) -> i32 {
        self.selective_display
    }

    #[cfg(test)]
    pub(crate) fn tab_width(self) -> i32 {
        self.tab_width
    }

    #[cfg(test)]
    pub(crate) fn accessible_end(self) -> i64 {
        self.accessible_end
    }
}
