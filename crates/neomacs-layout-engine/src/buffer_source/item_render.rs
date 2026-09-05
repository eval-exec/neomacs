//! Buffer source item rendering orchestration.

use crate::buffer_source::char_render::render_source_char_and_apply;
use crate::buffer_source::face_resolution::{
    BufferSourceItemLayoutResolutionContext, DisplaySourceNobreakHint,
};
use crate::buffer_source::item_append::BufferSourceRowAppendContext;
use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::buffer_source::loop_state::BufferSourceLoopMutableState;
use crate::buffer_source::row_lifecycle::{
    BufferSourceLineBreakRenderRequest, BufferSourceSelectiveDisplayTailRenderOutcome,
    BufferSourceSelectiveDisplayTailRenderRequest,
};
use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
use crate::buffer_source::text_run::BufferSourceTextRunRenderRequest;
use crate::buffer_source::walk::BufferSourceWalk;
use crate::display_face_ref::render_face_ref_id;
use crate::display_item::DisplayItemKind;
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::transition::DisplayRowTransitionContinuation;
use crate::display_source::DisplaySourceStepChar;
use crate::display_source::DisplaySourceStepItem;
use crate::neovm_bridge::LayoutBufferView;
use crate::types::WindowParams;
use neomacs_display_protocol::types::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceItemRenderOutcome {
    Rendered,
    ContinueBufferWalk,
    Stop,
}

impl BufferSourceItemRenderOutcome {
    pub(crate) fn should_break(self) -> bool {
        matches!(self, Self::Stop)
    }
}

/// The Renderable element arm's body.
///
/// P4.8(c): the request used to carry `loop_context` AND twelve values copied
/// straight back out of it, plus a `glyph_y_offset` that was only ever the
/// literal 0.0. Those are gone; the arm reads the loop context directly, and
/// only the references that are genuinely not window-invariant remain fields.
#[derive(Clone, Copy)]
pub(crate) struct BufferSourceItemRenderRequest<'a> {
    layout_resolution_context: BufferSourceItemLayoutResolutionContext<'a>,
    loop_context: BufferSourceLoopRequestContext,
    text: &'a [u8],
    append_surface: &'a DisplayRowAppendSurface,
    active_face_state: &'a DisplayRowActiveFaceState,
    params: &'a WindowParams,
}

/// Buffer glyphs sit on the row baseline the geometry already establishes, so
/// the element arm never offsets them. It was a struct field carrying this
/// constant through three request types.
const GLYPH_Y_OFFSET: f32 = 0.0;

impl<'a> BufferSourceItemRenderRequest<'a> {
    pub(crate) fn from_loop_context(
        layout_resolution_context: BufferSourceItemLayoutResolutionContext<'a>,
        loop_context: BufferSourceLoopRequestContext,
        text: &'a [u8],
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
        params: &'a WindowParams,
    ) -> Self {
        Self {
            layout_resolution_context,
            loop_context,
            text,
            append_surface,
            active_face_state,
            params,
        }
    }

    pub(crate) fn render_and_apply<B: LayoutBufferView>(
        mut self,
        source_item: DisplaySourceStepItem,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        row_prelude_context: Option<BufferSourceRowPreludeRequestContext>,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> bool {
        let source_step_char = source_item.source_step_char();
        let box_vertical_edges = source_item.item().box_vertical_edges;
        let line_spacing = match source_item.item().kind {
            DisplayItemKind::RowBreak(row_break) => row_break.line_spacing,
            _ => crate::display_item::DisplayLineSpacingPolicy::Inherit,
        };
        let mut state = state;
        let selective_display_outcome = self.render_selective_display_tail_for_context(
            &mut state,
            source_walk,
            source_step_char,
            buffer,
        );
        if selective_display_outcome.should_break() {
            return false;
        }
        if selective_display_outcome.should_continue_buffer_walk() {
            return true;
        }

        let is_explicit_line_break = source_item.is_explicit_line_break();
        let end_byte_idx = source_item.source_end_byte_idx();
        if is_explicit_line_break {
            if let Some(end_byte_idx) = end_byte_idx {
                state.progress.set_byte_idx(end_byte_idx);
            }
            if self
                .render_line_break_for_context(
                    &mut state,
                    source_walk,
                    source_step_char,
                    box_vertical_edges,
                    line_spacing,
                    buffer,
                )
                .should_break()
            {
                return false;
            }
            return true;
        }

        let outcome = self.render_text_item_and_apply(
            source_item,
            source_walk,
            buffer,
            row_prelude_context,
            state,
        );
        !outcome.should_break()
    }

    fn render_text_item_and_apply<B: LayoutBufferView>(
        self,
        source_item: DisplaySourceStepItem,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        row_prelude_context: Option<BufferSourceRowPreludeRequestContext>,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> BufferSourceItemRenderOutcome {
        self.render_prepared_source_item_and_apply(
            source_item,
            source_walk,
            buffer,
            row_prelude_context,
            state,
        )
    }

    fn render_selective_display_tail_for_context<B: LayoutBufferView>(
        &mut self,
        state: &mut BufferSourceLoopMutableState<'_, '_, '_>,
        source_walk: &mut BufferSourceWalk<'_, B>,
        source_step_char: DisplaySourceStepChar,
        buffer: &B,
    ) -> BufferSourceSelectiveDisplayTailRenderOutcome {
        let request = self.loop_context.selective_display_tail_request(
            source_step_char,
            self.text,
            state.surface.append_surface,
            self.active_face_state,
            GLYPH_Y_OFFSET,
        );
        self.render_selective_display_tail(state, source_walk, request, buffer)
    }

    fn render_selective_display_tail<B: LayoutBufferView>(
        &mut self,
        state: &mut BufferSourceLoopMutableState<'_, '_, '_>,
        source_walk: &mut BufferSourceWalk<'_, B>,
        request: BufferSourceSelectiveDisplayTailRenderRequest<'_>,
        buffer: &B,
    ) -> BufferSourceSelectiveDisplayTailRenderOutcome {
        request.render_if_needed_and_apply(source_walk, buffer, state.reborrow())
    }

    fn render_line_break_for_context<B: LayoutBufferView>(
        &mut self,
        state: &mut BufferSourceLoopMutableState<'_, '_, '_>,
        source_walk: &mut BufferSourceWalk<'_, B>,
        source_char: DisplaySourceStepChar,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
        line_spacing: crate::display_item::DisplayLineSpacingPolicy,
        buffer: &B,
    ) -> DisplayRowTransitionContinuation {
        let request = self
            .loop_context
            .line_break_request(
                source_char,
                self.text,
                self.append_surface,
                self.active_face_state,
            )
            .with_box_vertical_edges(box_vertical_edges)
            .with_line_spacing(line_spacing);
        self.render_line_break(state, source_walk, request, buffer)
    }

    fn render_line_break<B: LayoutBufferView>(
        &mut self,
        state: &mut BufferSourceLoopMutableState<'_, '_, '_>,
        source_walk: &mut BufferSourceWalk<'_, B>,
        request: BufferSourceLineBreakRenderRequest<'_>,
        buffer: &B,
    ) -> DisplayRowTransitionContinuation {
        request.render_and_apply(source_walk, buffer, state.reborrow())
    }

    fn render_prepared_source_item_and_apply<B: LayoutBufferView>(
        self,
        mut source_item: DisplaySourceStepItem,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        row_prelude_context: Option<BufferSourceRowPreludeRequestContext>,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> BufferSourceItemRenderOutcome {
        let BufferSourceLoopMutableState {
            invisible_text_checkpoint,
            mut progress,
            source_render,
            row_build,
            row_carryover,
            row_source_start,
            face_scan,
            row_y_positions,
            cursor_info,
            face_ids,
            beyond_accessible_end_line_prefix,
            surface,
        } = state;
        let mut source_render = source_render;
        let predecessor_row_extend = row_build
            .row_extend
            .value_on(row_build.row_geometry)
            .copied();
        // The unsubstituted buffer char + active nobreak policy, used by the
        // nbsp / nobreak-hyphen highlight branch in face resolution. Captured
        // before the mutable `item_mut()` borrow below.
        let nobreak_hint = DisplaySourceNobreakHint::new(
            source_item.source_step_char().ch(),
            self.params.nobreak_char_display,
        );
        let active_face_state = self
            .layout_resolution_context
            .resolve_source_item_layout_for_active_face(
                &mut source_render,
                face_ids,
                row_build.row_geometry,
                self.active_face_state,
                source_item.item_mut(),
                nobreak_hint,
            );
        let item_face_id = render_face_ref_id(source_item.item().face, active_face_state.face_id());
        if item_face_id == active_face_state.face_id() {
            // Layout-only face transforms (height, escape-glyph and nobreak
            // highlighting) happen after source-property resolution. Preserve
            // their complete identity here before this item can be split and
            // its suffix queued for a later row/iteration.
            source_walk.remember_resolved_source_face_if_absent(
                active_face_state.face_id(),
                active_face_state.resolved_face(),
            );
        }
        let resolved_item_face = source_walk
            .resolved_source_face(item_face_id)
            .cloned()
            .map(|face| (item_face_id, face));
        let row_extend_fill = resolved_item_face
            .as_ref()
            .and_then(|(face_id, face)| {
                face.extend.then(|| {
                    crate::display_row::face_state::DisplayRowExtendFace::new(
                        Color::from_pixel(face.bg),
                        *face_id,
                        active_face_state.metrics(),
                    )
                })
            })
            .or_else(|| active_face_state.row_extend_fill());
        if let Some(fill) = row_extend_fill {
            row_build
                .row_extend
                .activate(row_build.row_geometry.current_row_marker(), fill);
        } else {
            row_build.row_extend.clear();
        }
        let mut buffer_row_append_context = BufferSourceRowAppendContext::from_active_face_row(
            buffer,
            self.loop_context.buffer_id(),
            self.append_surface,
            &active_face_state,
            GLYPH_Y_OFFSET,
            self.loop_context.char_height(),
            face_ids.clone(),
        );
        if let Some((face_id, face)) = resolved_item_face {
            buffer_row_append_context =
                buffer_row_append_context.with_resolved_item_face(face_id, face);
        }
        let append_position = progress.row_position();
        let append_geometry = *row_build.row_geometry;
        let text_run_request = BufferSourceTextRunRenderRequest::new(
            self.loop_context.text_start_byte(),
            self.loop_context.point_charpos(),
            self.append_surface.right_edge(),
            append_position,
            append_geometry,
        );

        if let Some(outcome) = text_run_request.render_if_fits_and_apply(
            source_item.clone(),
            &active_face_state,
            &buffer_row_append_context,
            cursor_info,
            row_carryover.trailing_whitespace,
            row_carryover.word_wrap,
            predecessor_row_extend,
            &mut source_render,
            &mut progress,
        ) {
            return outcome;
        }

        if let Some(prefix) = text_run_request.prefix_to_fit(
            &source_item,
            self.params.wrap_mode,
            &buffer_row_append_context,
            &mut source_render,
        ) {
            // Consume only this prefix: the producer is reseated at the first
            // unfitting character, so the next element is produced from there.
            // The tail used to be queued purely for the next iteration to pop
            // it, find it does not fit, and have the truncation skip discard it.
            if let Some(resume_charpos) = prefix.source_end_charpos() {
                source_walk.consume_prefix_to(resume_charpos);
            }
            return text_run_request.render_and_apply(
                prefix,
                &active_face_state,
                &buffer_row_append_context,
                cursor_info,
                row_carryover.trailing_whitespace,
                row_carryover.word_wrap,
                predecessor_row_extend,
                &mut source_render,
                &mut progress,
            );
        }

        // Neither whole-run path could take this run, so it is rendered one
        // character at a time from here on. Tell the producer to stop batching
        // for the rest of it: otherwise every character re-reads and re-measures
        // the whole remaining run, which measured 79x the run measurements on a
        // 2000-character wrapped line. The hint expires by position.
        if let Some(end_charpos) = source_item.source_end_charpos() {
            source_walk.request_char_granularity_until(end_charpos);
        }

        render_source_char_and_apply(
            self.loop_context,
            self.text,
            self.params,
            source_item,
            source_walk,
            buffer,
            row_prelude_context,
            &active_face_state,
            &buffer_row_append_context,
            predecessor_row_extend,
            BufferSourceLoopMutableState::new(
                invisible_text_checkpoint,
                progress,
                source_render,
                row_build,
                row_source_start,
                row_carryover,
                face_scan,
                row_y_positions,
                cursor_info,
                face_ids,
                surface,
            )
            .with_beyond_accessible_end_line_prefix_capture_state(
                beyond_accessible_end_line_prefix,
            ),
        )
    }
}
