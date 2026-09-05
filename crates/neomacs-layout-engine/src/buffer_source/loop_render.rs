//! Buffer text visible-loop rendering.

use crate::buffer_source::face_resolution::*;
use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::buffer_source::loop_state::BufferSourceLoopMutableState;
use crate::buffer_source::render::BufferSourceRenderRequest;
use crate::buffer_source::row_lifecycle::{
    BufferSourceHscrollSkipRenderContext, BufferSourceInvisibleTextRenderContext,
    BufferSourceInvisibleTextRenderOutcome,
};
use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
use crate::buffer_source::walk::*;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::transition::DisplayRowTransitionContinuation;
use crate::neovm_bridge::LayoutBufferView;
use crate::types::WindowParams;

impl<'rows, 'emit, 'surface> BufferSourceLoopMutableState<'rows, 'emit, 'surface> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_visible_steps<'request, B: LayoutBufferView>(
        &mut self,
        loop_context: BufferSourceLoopRequestContext,
        source_walk: &mut BufferSourceWalk<'request, B>,
        row_prelude_context: BufferSourceRowPreludeRequestContext,
        face_resolution_context: BufferSourceFaceResolutionContext<'request, B>,
        text: &'request [u8],
        params: &'request WindowParams,
        active_face_state: &mut DisplayRowActiveFaceState,
        buffer: &B,
    ) where
        'surface: 'request,
    {
        // P4.8(b): walk-scoped, because a refusal window is a claim about
        // absolute buffer positions this walk has already classified. It
        // needs no invalidation — point does not move during a walk, and a
        // window the walk has passed can never cover a later position.
        let mut route_refusals = crate::buffer_source::row_route::RouteRefusalWindow::default();
        while self.progress.byte_idx() < text.len()
            && self
                .row_build
                .row_geometry
                .current_row_is_visible(loop_context.row_visibility_limit())
        {
            self.render_row_prelude(row_prelude_context, params, active_face_state, buffer);

            let invisible_text_outcome = self.render_invisible_text_for_context(
                loop_context,
                source_walk,
                text,
                active_face_state,
                buffer,
            );
            match invisible_text_outcome {
                BufferSourceInvisibleTextRenderOutcome::Visible => {}
                BufferSourceInvisibleTextRenderOutcome::RenderBoundaryOverlayStrings => {
                    if !self.render_next_source_item_for_context(
                        loop_context,
                        source_walk,
                        row_prelude_context,
                        face_resolution_context,
                        text,
                        params,
                        active_face_state,
                        buffer,
                    ) {
                        break;
                    }
                    continue;
                }
                BufferSourceInvisibleTextRenderOutcome::HiddenSpanApplied => continue,
            }

            if self.row_carryover.hscroll_skip.should_skip() {
                if self
                    .render_hscroll_skip_for_context(
                        loop_context,
                        source_walk,
                        text,
                        active_face_state,
                    )
                    .should_break()
                {
                    break;
                }
                continue;
            }

            self.render_face_checkpoint_for_context(face_resolution_context, active_face_state);

            // The two cooperating render paths, tried in order. The route is
            // the WHOLE-ROW fast path: when the classifier can prove a row
            // plain, it plans and commits the row body in one pass through
            // render_plan.rs, and hands the newline back to the line-break
            // lifecycle below. It does NOT go through the element arm — the
            // two paths share the row lifecycle, not the renderer.
            //
            // Coverage is capability-bounded, not total (~44% of attempts;
            // display tables, point rows, box faces and word wrap genuinely
            // cannot route), so the element arm below is the GENERAL path and
            // renders everything the route refuses.
            use crate::buffer_source::row_route::PlainRowRouteOutcome;
            match self.try_render_plain_row_via_item_renderer(
                crate::buffer_source::row_route::PlainRowRouteRequest::new(
                    loop_context,
                    face_resolution_context,
                    text,
                    params,
                ),
                source_walk,
                active_face_state,
                &mut route_refusals,
            ) {
                PlainRowRouteOutcome::Rendered => continue,
                PlainRowRouteOutcome::Stopped => break,
                PlainRowRouteOutcome::NotRouted => {}
            }

            if !self.render_next_source_item_for_context(
                loop_context,
                source_walk,
                row_prelude_context,
                face_resolution_context,
                text,
                params,
                active_face_state,
                buffer,
            ) {
                break;
            }
        }

        // A trailing newline begins the next visual row at the same moment it
        // consumes the final source byte.  That row is still a real, visible
        // EOB row, but the byte-driven loop above cannot enter once more to
        // render its prelude.  Complete the WHOLE pending prelude before the
        // tail captures point; line numbers reserve a blank prefix beyond ZV,
        // while line-prefix keeps the same row-begin lifecycle as every other
        // visual row.
        if self.progress.byte_idx() >= text.len()
            && self
                .row_build
                .row_geometry
                .current_row_is_visible(loop_context.row_visibility_limit())
        {
            self.row_carryover.line_numbers.mark_beyond_accessible_end();
            self.render_row_prelude(row_prelude_context, params, active_face_state, buffer);
        }
    }

    pub(crate) fn render_row_prelude<B: LayoutBufferView>(
        &mut self,
        context: BufferSourceRowPreludeRequestContext,
        params: &WindowParams,
        active_face_state: &DisplayRowActiveFaceState,
        buffer: &B,
    ) {
        self.render_pending_line_number_prefix(context);

        let wrap_prefix = self.row_carryover.prefix_request.is_wrap();
        let line_prefix_checkpoint =
            (!wrap_prefix).then(|| self.source_render.capture_glyph_checkpoint());
        let row_position = if wrap_prefix {
            // GNU computes TABs inside a wrap-prefix from the current screen
            // row, not from the continued physical line.
            self.progress.row_position().on_screen_line_tab_grid()
        } else {
            self.progress.row_position()
        };
        let prefix_start_x = row_position.x_px();
        let charpos = self.progress.charpos();
        let prefix_end_x = {
            let (x, col) = self.progress.row_progress_mut().coordinates_mut();
            context
                .line_prefix_request(
                    self.surface.append_surface,
                    self.row_build.row_geometry,
                    active_face_state,
                    0.0,
                    row_position,
                    params,
                )
                .render_requested_with_source_state_and_apply(
                    self.row_carryover.prefix_request,
                    &mut self.source_render,
                    buffer,
                    charpos,
                    self.face_ids,
                    x,
                    col,
                );
            *x
        };
        if wrap_prefix {
            self.progress
                .record_wrap_prefix_width((prefix_end_x - prefix_start_x).max(0.0));
        } else if let (Some(checkpoint), Some(row)) = (
            line_prefix_checkpoint,
            self.source_render.current_row_snapshot(),
        ) {
            self.beyond_accessible_end_line_prefix.replace(
                crate::buffer_source::end_of_buffer_rows::BeyondAccessibleEndLinePrefix::capture(
                    checkpoint, row,
                ),
            );
        }
    }

    fn render_pending_line_number_prefix(&mut self, context: BufferSourceRowPreludeRequestContext) {
        context
            .line_number_prefix_request()
            .render_pending_with_source_state(
                self.row_carryover.line_numbers,
                &mut self.source_render,
                self.face_ids,
                self.row_build.row_geometry,
                self.face_scan,
            );
    }

    fn render_invisible_text_for_context<'request, B: LayoutBufferView>(
        &mut self,
        loop_context: BufferSourceLoopRequestContext,
        source_walk: &mut BufferSourceWalk<'_, B>,
        text: &'request [u8],
        active_face_state: &'request DisplayRowActiveFaceState,
        buffer: &B,
    ) -> BufferSourceInvisibleTextRenderOutcome
    where
        'surface: 'request,
    {
        let request = loop_context.invisible_text_request(
            text,
            self.surface.append_surface,
            active_face_state,
            0.0,
        );
        self.render_invisible_text_at_checkpoint(source_walk, request, buffer)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_next_source_item_for_context<'request, 'face, B: LayoutBufferView>(
        &mut self,
        loop_context: BufferSourceLoopRequestContext,
        source_walk: &mut BufferSourceWalk<'request, B>,
        row_prelude_context: BufferSourceRowPreludeRequestContext,
        face_resolution_context: BufferSourceFaceResolutionContext<'request, B>,
        text: &'request [u8],
        params: &'request WindowParams,
        active_face_state: &'face DisplayRowActiveFaceState,
        buffer: &B,
    ) -> bool
    where
        'surface: 'request,
    {
        BufferSourceRenderRequest::new(
            loop_context,
            text,
            params,
            active_face_state,
            self.reborrow(),
        )
        .with_row_prelude_context(row_prelude_context)
        .render_next_and_apply(source_walk, face_resolution_context, buffer)
    }

    fn render_invisible_text_at_checkpoint<B: LayoutBufferView>(
        &mut self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        request: BufferSourceInvisibleTextRenderContext<'_>,
        buffer: &B,
    ) -> BufferSourceInvisibleTextRenderOutcome {
        request.render_at_checkpoint_and_apply(source_walk, buffer, self.reborrow())
    }

    fn render_hscroll_skip_for_context<'request, B: LayoutBufferView>(
        &mut self,
        loop_context: BufferSourceLoopRequestContext,
        source_walk: &mut BufferSourceWalk<'_, B>,
        text: &'request [u8],
        active_face_state: &'request DisplayRowActiveFaceState,
    ) -> DisplayRowTransitionContinuation
    where
        'surface: 'request,
    {
        let request =
            loop_context.hscroll_skip_request(text, self.surface.append_surface, active_face_state);
        self.render_hscroll_skip(source_walk, request)
    }

    fn render_hscroll_skip<B: LayoutBufferView>(
        &mut self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        request: BufferSourceHscrollSkipRenderContext<'_>,
    ) -> DisplayRowTransitionContinuation {
        request.render_next_and_apply(source_walk, self.reborrow())
    }

    fn render_face_checkpoint_for_context<B: LayoutBufferView>(
        &mut self,
        face_resolution_context: BufferSourceFaceResolutionContext<'_, B>,
        active_face_state: &mut DisplayRowActiveFaceState,
    ) {
        face_resolution_context.resolve_at_checkpoint_with_source_state(
            &mut self.source_render.reborrow(),
            self.face_scan,
            self.face_ids,
            active_face_state,
            self.row_build.row_geometry,
            self.row_build.row_extend,
            self.row_build.box_face,
            self.progress.row_progress().x(),
            self.progress.charpos(),
        );
    }
}
