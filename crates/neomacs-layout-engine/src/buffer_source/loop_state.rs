//! Shared mutable state for buffer text visible-loop rendering.
//!
//! The state is grouped into sub-structs by what the loop does with each
//! group — the row under construction, hit capture, what a row break carries
//! across, and the surface-scoped context — so that a site naming one group
//! says which part of the walk it touches. Each group reborrows like the
//! whole, so handing a group on is the same move as handing the state on.

use crate::display_cursor::CursorCaptureState;
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::geometry::{
    DisplayRowExtendState, DisplayRowFlags, DisplayRowGeometryState, DisplayRowYPositions,
};
use crate::display_row::lisp_string::DisplayRowPrefixRequest;
use crate::display_row::overlay_string::BufferOverlayStringTextRowRenderContext;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::transition::DisplayRowTransitionRenderState;
use crate::display_row::walk_state::{
    BoxFaceRowState, FaceScanCheckpoint, HitRowRangeTracker, HorizontalScrollSkipState,
    InvisibleTextScanCheckpoint, LineNumberRenderState, TrailingWhitespaceRenderState,
    WordWrapRenderState,
};
use crate::display_source_progress::DisplaySourceProgressState;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::hit_test::HitRow;

/// The glyph row currently under construction.
///
/// All four are scoped to one row and are finalized together: a row
/// transition installs `row_geometry`/`row_flags`, flushes the
/// extend-to-end-of-line fill recorded in `row_extend`, and closes any box
/// face whose left edge `box_face` pinned on this row. Reading one of them
/// against a different row than the others is the bug this bundle names.
pub(crate) struct BufferSourceRowBuildState<'emit> {
    pub(crate) row_geometry: &'emit mut DisplayRowGeometryState,
    pub(crate) row_flags: &'emit mut DisplayRowFlags,
    pub(crate) row_extend: &'emit mut DisplayRowExtendState,
    pub(crate) box_face: &'emit mut BoxFaceRowState,
}

/// Hit-test capture for the window being laid out.
///
/// `hit_row_range` holds the charpos the current row started at; it is what
/// closes an entry appended to the frame-scoped `hit_rows`. Neither is
/// meaningful without the other, and a row emitted from one while the range
/// came from another is an off-by-one-row hit map.
pub(crate) struct BufferSourceHitCaptureState<'emit> {
    pub(crate) hit_rows: &'emit mut Vec<HitRow>,
    pub(crate) hit_row_range: &'emit mut HitRowRangeTracker,
}

/// The walk state a row transition carries across a row break.
///
/// These are exactly the borrows `DisplayRowTransitionRenderState` takes, so
/// every transition site builds its render state from this one bundle
/// (`render_state`) instead of naming five fields. They correlate because the
/// transition is the only thing that updates them all: it re-requests the
/// continuation prefix, advances the line number, re-arms the hscroll skip,
/// and resets the word-wrap candidate and trailing-whitespace tracker for the
/// new row.
pub(crate) struct BufferSourceRowCarryoverState<'emit> {
    pub(crate) prefix_request: &'emit mut DisplayRowPrefixRequest,
    pub(crate) line_numbers: &'emit mut LineNumberRenderState,
    pub(crate) hscroll_skip: &'emit mut HorizontalScrollSkipState,
    pub(crate) word_wrap: &'emit mut WordWrapRenderState,
    pub(crate) trailing_whitespace: &'emit mut TrailingWhitespaceRenderState,
}

/// The append surface and the overlay-string render context.
///
/// Both are borrowed from the surface for the whole loop and neither is
/// mutated by it: they answer "where does this row end / what does a glyph
/// append against" and "which overlay strings does this window render". They
/// share the `'surface` lifetime, which is what separates them from every
/// other member of the loop state.
#[derive(Clone, Copy)]
pub(crate) struct BufferSourceSurfaceContext<'surface> {
    pub(crate) append_surface: &'surface DisplayRowAppendSurface,
    pub(crate) overlay_context: BufferOverlayStringTextRowRenderContext<'surface>,
}

impl<'emit> BufferSourceRowBuildState<'emit> {
    pub(crate) fn new(
        row_geometry: &'emit mut DisplayRowGeometryState,
        row_flags: &'emit mut DisplayRowFlags,
        row_extend: &'emit mut DisplayRowExtendState,
        box_face: &'emit mut BoxFaceRowState,
    ) -> Self {
        Self {
            row_geometry,
            row_flags,
            row_extend,
            box_face,
        }
    }

    pub(crate) fn reborrow(&mut self) -> BufferSourceRowBuildState<'_> {
        BufferSourceRowBuildState {
            row_geometry: self.row_geometry,
            row_flags: self.row_flags,
            row_extend: self.row_extend,
            box_face: self.box_face,
        }
    }
}

impl<'emit> BufferSourceHitCaptureState<'emit> {
    pub(crate) fn new(
        hit_rows: &'emit mut Vec<HitRow>,
        hit_row_range: &'emit mut HitRowRangeTracker,
    ) -> Self {
        Self {
            hit_rows,
            hit_row_range,
        }
    }

    pub(crate) fn reborrow(&mut self) -> BufferSourceHitCaptureState<'_> {
        BufferSourceHitCaptureState {
            hit_rows: self.hit_rows,
            hit_row_range: self.hit_row_range,
        }
    }
}

impl<'emit> BufferSourceRowCarryoverState<'emit> {
    pub(crate) fn new(
        prefix_request: &'emit mut DisplayRowPrefixRequest,
        line_numbers: &'emit mut LineNumberRenderState,
        hscroll_skip: &'emit mut HorizontalScrollSkipState,
        word_wrap: &'emit mut WordWrapRenderState,
        trailing_whitespace: &'emit mut TrailingWhitespaceRenderState,
    ) -> Self {
        Self {
            prefix_request,
            line_numbers,
            hscroll_skip,
            word_wrap,
            trailing_whitespace,
        }
    }

    pub(crate) fn reborrow(&mut self) -> BufferSourceRowCarryoverState<'_> {
        BufferSourceRowCarryoverState {
            prefix_request: self.prefix_request,
            line_numbers: self.line_numbers,
            hscroll_skip: self.hscroll_skip,
            word_wrap: self.word_wrap,
            trailing_whitespace: self.trailing_whitespace,
        }
    }

    /// The render state a row transition runs with. `has_prefix` is the only
    /// piece the loop state does not own: it is a property of the call site's
    /// context, not of the walk.
    pub(crate) fn render_state(&mut self, has_prefix: bool) -> DisplayRowTransitionRenderState<'_> {
        DisplayRowTransitionRenderState::new(
            self.prefix_request,
            has_prefix,
            self.line_numbers,
            self.hscroll_skip,
            self.word_wrap,
            self.trailing_whitespace,
        )
    }
}

impl<'surface> BufferSourceSurfaceContext<'surface> {
    pub(crate) fn new(
        append_surface: &'surface DisplayRowAppendSurface,
        overlay_context: BufferOverlayStringTextRowRenderContext<'surface>,
    ) -> Self {
        Self {
            append_surface,
            overlay_context,
        }
    }
}

pub(crate) struct BufferSourceLoopMutableState<'rows, 'emit, 'surface> {
    pub(crate) invisible_text_checkpoint: &'emit mut InvisibleTextScanCheckpoint,
    pub(crate) progress: DisplaySourceProgressState<'emit>,
    pub(crate) source_render: TextRowSourceRenderState<'emit>,
    pub(crate) row_build: BufferSourceRowBuildState<'emit>,
    pub(crate) hit_capture: BufferSourceHitCaptureState<'emit>,
    pub(crate) row_carryover: BufferSourceRowCarryoverState<'emit>,
    pub(crate) face_scan: &'emit mut FaceScanCheckpoint,
    pub(crate) row_y_positions: &'rows mut DisplayRowYPositions,
    pub(crate) cursor_info: &'emit mut CursorCaptureState,
    pub(crate) face_ids: &'emit mut FrameFaceAttempt,
    pub(crate) surface: BufferSourceSurfaceContext<'surface>,
}

impl<'rows, 'emit, 'surface> BufferSourceLoopMutableState<'rows, 'emit, 'surface> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        invisible_text_checkpoint: &'emit mut InvisibleTextScanCheckpoint,
        progress: DisplaySourceProgressState<'emit>,
        source_render: TextRowSourceRenderState<'emit>,
        row_build: BufferSourceRowBuildState<'emit>,
        hit_capture: BufferSourceHitCaptureState<'emit>,
        row_carryover: BufferSourceRowCarryoverState<'emit>,
        face_scan: &'emit mut FaceScanCheckpoint,
        row_y_positions: &'rows mut DisplayRowYPositions,
        cursor_info: &'emit mut CursorCaptureState,
        face_ids: &'emit mut FrameFaceAttempt,
        surface: BufferSourceSurfaceContext<'surface>,
    ) -> Self {
        Self {
            invisible_text_checkpoint,
            progress,
            source_render,
            row_build,
            hit_capture,
            row_carryover,
            face_scan,
            row_y_positions,
            cursor_info,
            face_ids,
            surface,
        }
    }

    pub(crate) fn reborrow(&mut self) -> BufferSourceLoopMutableState<'_, '_, 'surface> {
        BufferSourceLoopMutableState {
            invisible_text_checkpoint: self.invisible_text_checkpoint,
            progress: self.progress.reborrow(),
            source_render: self.source_render.reborrow(),
            row_build: self.row_build.reborrow(),
            hit_capture: self.hit_capture.reborrow(),
            row_carryover: self.row_carryover.reborrow(),
            face_scan: self.face_scan,
            row_y_positions: self.row_y_positions,
            cursor_info: self.cursor_info,
            face_ids: self.face_ids,
            surface: self.surface,
        }
    }
}
