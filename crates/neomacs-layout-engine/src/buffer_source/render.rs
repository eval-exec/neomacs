//! Buffer text source consumption with replacement application.

use crate::buffer_source::consumption::BufferSourceConsumedItem;
use crate::buffer_source::display_property_render::{
    BufferDisplayPropertyTextReplacementApplyOutcome,
    BufferDisplayPropertyTextReplacementRenderContext,
    BufferDisplayPropertyTextReplacementRenderState,
};
use crate::buffer_source::face_resolution::BufferSourceFaceResolutionContext;
use crate::buffer_source::face_resolution::BufferSourceItemLayoutResolutionContext;
use crate::buffer_source::item_render::BufferSourceItemRenderRequest;
use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::buffer_source::loop_state::BufferSourceLoopMutableState;
use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
use crate::buffer_source::text_source::BufferOverlayStringsItem;
use crate::buffer_source::walk::BufferSourceWalk;
use crate::display_face_ref::render_face_ref_id;
use crate::display_item::BufferDisplayPropertyReplacementItem;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::overlay_string::OverlayStringRenderPositions;
use crate::display_row::replacement::{
    DisplayPropertyReplacementStringRender, DisplayReplacementStringRowStop,
};
use crate::display_row::transition::{
    DisplayRowOverflowTransitionPlan, DisplayRowTextWindowEmitContext,
    DisplayRowTransitionContinuation, VisualWrapBreak,
};
use crate::display_row::walk_state::TextRowTransitionStatePolicy;
use crate::display_source::DisplaySourceStepChar;
use crate::display_source::DisplaySourceStepItem;
use crate::neovm_bridge::LayoutBufferView;
use crate::types::{LayoutCharPos0, WindowParams};

pub(crate) fn emit_nested_source_visual_wrap(
    loop_context: BufferSourceLoopRequestContext,
    state: BufferSourceLoopMutableState<'_, '_, '_>,
) -> DisplayRowTransitionContinuation {
    let BufferSourceLoopMutableState {
        mut progress,
        mut source_render,
        row_build,
        mut row_carryover,
        row_source_start,
        face_scan,
        row_y_positions,
        surface,
        ..
    } = state;
    let row_end_x = progress.row_progress().x();
    progress.continue_physical_line_after_visual_row(row_end_x, loop_context.content_x());
    {
        let box_vertical_edges = source_render.trailing_box_run_terminal();
        source_render.extend_face_to_end_of_line(
            row_build.row_extend,
            row_build.row_geometry,
            row_end_x,
            surface.append_surface.right_edge(),
            loop_context.frame_background(),
            box_vertical_edges,
        );
    }
    *progress.row_progress_mut().x_mut() = loop_context.content_x();
    row_build.row_extend.clear();

    let charpos = progress.charpos();
    let next_row_start = LayoutCharPos0::new(charpos);
    // A nested display source that runs past the right edge is broken
    // mid-element, the branch where GNU produces IT_CONTINUATION
    // (src/xdisp.c:26421-26432).
    let transition = DisplayRowOverflowTransitionPlan::visual_wrap(
        VisualWrapBreak::MidElement,
        TextRowTransitionStatePolicy::visual_wrap(),
    );
    let row_position = progress.row_position();
    let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
        loop_context.row_geometry_defaults(),
        loop_context.display_text_row_base(),
        row_y_positions,
        loop_context.max_rows(),
        row_build.row_geometry,
        row_build.row_flags,
        loop_context.row_limit(),
        &mut source_render,
    )
    .emit_overflow_then_row_start(
        transition,
        next_row_start,
        row_position,
        row_carryover.render_state(loop_context.has_prefix()),
        progress.row_progress_mut().col_mut(),
    );
    if row_transition.is_exhausted() {
        return DisplayRowTransitionContinuation::Exhausted;
    }

    row_source_start.advance_to(charpos);
    face_scan.invalidate();
    DisplayRowTransitionContinuation::after_visible_row_transition(
        row_transition,
        row_build.row_geometry,
        loop_context.row_visibility_limit(),
    )
}

pub(crate) struct BufferSourceRenderRequest<'rows, 'request, 'emit, 'surface, 'face> {
    loop_context: BufferSourceLoopRequestContext,
    text: &'request [u8],
    params: &'request WindowParams,
    active_face_state: &'face DisplayRowActiveFaceState,
    state: BufferSourceLoopMutableState<'rows, 'emit, 'surface>,
    row_prelude_context: Option<BufferSourceRowPreludeRequestContext>,
}

impl<'rows, 'request, 'emit, 'surface, 'face>
    BufferSourceRenderRequest<'rows, 'request, 'emit, 'surface, 'face>
{
    pub(crate) fn new(
        loop_context: BufferSourceLoopRequestContext,
        text: &'request [u8],
        params: &'request WindowParams,
        active_face_state: &'face DisplayRowActiveFaceState,
        state: BufferSourceLoopMutableState<'rows, 'emit, 'surface>,
    ) -> Self {
        Self {
            loop_context,
            text,
            params,
            active_face_state,
            state,
            row_prelude_context: None,
        }
    }

    pub(crate) fn with_row_prelude_context(
        mut self,
        row_prelude_context: BufferSourceRowPreludeRequestContext,
    ) -> Self {
        self.row_prelude_context = Some(row_prelude_context);
        self
    }

    pub(crate) fn render_next_and_apply<B: LayoutBufferView>(
        mut self,
        source_walk: &mut BufferSourceWalk<'request, B>,
        face_resolution_context: BufferSourceFaceResolutionContext<'request, B>,
        buffer: &B,
    ) -> bool
    where
        'surface: 'request,
    {
        let layout_resolution_context =
            face_resolution_context.source_item_layout_resolution_context();
        let Some(consumed_item) = source_walk.consume_source_item_for_render(
            &mut self.state.progress,
            face_resolution_context,
            self.state.face_ids,
            &mut self.state.source_render.reborrow(),
            self.state.row_build.row_geometry,
            self.state.surface.append_surface,
            self.active_face_state,
        ) else {
            return false;
        };

        match consumed_item {
            BufferSourceConsumedItem::DisplayPropertyReplacement(replacement) => self
                .consume_replacement(source_walk, layout_resolution_context, replacement, buffer),
            BufferSourceConsumedItem::Renderable(source_item) => {
                self.render_source_item(source_walk, layout_resolution_context, source_item, buffer)
            }
            BufferSourceConsumedItem::OverlayStrings(strings) => {
                self.render_overlay_strings(strings, buffer)
            }
        }
    }

    /// Append the overlay strings the producer anchored at this position.
    ///
    /// The producer decided WHERE they belong and in WHICH order (GNU
    /// `compare_overlay_entries`); this arm owns only the append, which stays a
    /// per-string session because a string can break rows, clip against the
    /// right edge and carry its own `cursor` property. The element has insertion
    /// semantics, so the walk position is untouched and the next production is
    /// the buffer character at the same anchor.
    fn render_overlay_strings<B: LayoutBufferView>(
        &mut self,
        strings: BufferOverlayStringsItem,
        buffer: &B,
    ) -> bool {
        let word_wrap_boundary = strings.word_wrap_boundary();
        if let Some(boundary) = word_wrap_boundary {
            // GNU xdisp.c display_line saves the complete iterator BEFORE the
            // first display element after whitespace.  The producer surfaces
            // an overlay before-string before its anchor character, so this is
            // the only point where source position, output metadata, and glyph
            // counts still all describe the boundary before that string.
            self.state.row_carryover.word_wrap.record_source_candidate(
                boundary.first(),
                self.state.progress.source_position(),
                &self.state.source_render,
                self.state.progress.row_position(),
                self.state
                    .row_build
                    .row_extend
                    .value_on(self.state.row_build.row_geometry)
                    .copied(),
            );
        }
        let positions = OverlayStringRenderPositions::from_attachment_and_layout_point(
            strings.anchor_charpos(),
            self.loop_context.point_charpos(),
        );
        let (x, col) = self.state.progress.row_progress_mut().coordinates_mut();
        let continuation = self
            .state
            .surface
            .overlay_context
            .render_produced_strings_at_text_row(
                buffer,
                positions,
                strings.strings(),
                strings.box_boundaries(),
                self.state.source_render.reborrow(),
                x,
                col,
                self.state.row_build.row_geometry,
                self.state.cursor_info,
                self.state.row_source_start,
                self.state.row_y_positions,
                self.state.face_ids,
                self.state.row_carryover.line_numbers,
                self.state.face_scan,
            );
        if let Some(boundary) = word_wrap_boundary {
            if continuation.should_break() {
                self.state
                    .row_carryover
                    .word_wrap
                    .reset_after_row_transition();
            } else {
                // The following buffer character sees the last displayed
                // string character as its predecessor, just as GNU's iterator
                // updates `may_wrap` while consuming the string stack.
                self.state
                    .row_carryover
                    .word_wrap
                    .allow_after_current_char(boundary.last());
            }
        }
        !continuation.should_break()
    }

    fn consume_replacement<B: LayoutBufferView>(
        mut self,
        source_walk: &mut BufferSourceWalk<'request, B>,
        layout_resolution_context: BufferSourceItemLayoutResolutionContext<'request>,
        replacement: BufferDisplayPropertyReplacementItem,
        buffer: &B,
    ) -> bool
    where
        'surface: 'request,
    {
        let replacement_context = BufferDisplayPropertyTextReplacementRenderContext::new(
            replacement,
            self.loop_context.text_start_byte(),
            self.text,
            self.loop_context.content_x(),
            self.params,
            0.0,
            self.loop_context.char_height(),
            self.active_face_state,
            self.state.progress.row_progress().x(),
            self.state.progress.row_position(),
        );
        match replacement_context.render_and_apply(
            buffer,
            BufferDisplayPropertyTextReplacementRenderState::new(
                self.state.source_render.reborrow(),
                self.state.face_ids,
                self.state.surface.append_surface,
                self.state.row_build.row_geometry,
                self.active_face_state,
            ),
            &mut self.state.progress,
            self.state.cursor_info,
            self.loop_context.point_charpos(),
        ) {
            BufferDisplayPropertyTextReplacementApplyOutcome::Applied => true,
            BufferDisplayPropertyTextReplacementApplyOutcome::String(session) => self
                .render_display_string_session(source_walk, buffer, &replacement_context, session),
            BufferDisplayPropertyTextReplacementApplyOutcome::Fallback(source_item) => {
                self.render_source_item(source_walk, layout_resolution_context, source_item, buffer)
            }
            BufferDisplayPropertyTextReplacementApplyOutcome::Stop => false,
        }
    }

    fn render_display_string_session<B: LayoutBufferView>(
        &mut self,
        source_walk: &mut BufferSourceWalk<'request, B>,
        buffer: &B,
        replacement_context: &BufferDisplayPropertyTextReplacementRenderContext<'_, '_>,
        mut session: DisplayPropertyReplacementStringRender,
    ) -> bool
    where
        'surface: 'request,
    {
        loop {
            let Some(outcome) = session.render_next_row(
                &mut self.state.source_render.reborrow(),
                self.state.face_ids,
                self.state.surface.append_surface,
                self.state.row_build.row_geometry,
                self.active_face_state,
                self.state.progress.row_position(),
            ) else {
                return false;
            };
            self.state
                .progress
                .apply_row_position(outcome.end_position());

            match outcome.stop() {
                DisplayReplacementStringRowStop::SourceExhausted => {
                    replacement_context.apply_completed_string(
                        session.finish(outcome.end_position()),
                        &mut self.state.progress,
                    );
                    return true;
                }
                DisplayReplacementStringRowStop::Clipped
                    if self.params.wrap_mode == crate::types::LineWrapMode::Truncate =>
                {
                    replacement_context.apply_completed_string(
                        session.finish(outcome.end_position()),
                        &mut self.state.progress,
                    );
                    return true;
                }
                DisplayReplacementStringRowStop::Clipped => {
                    if self.emit_display_string_visual_wrap(buffer).should_break() {
                        return false;
                    }
                }
                DisplayReplacementStringRowStop::RowBreak(line_break) => {
                    if !self.emit_display_string_row_break(source_walk, buffer, line_break) {
                        return false;
                    }
                    self.render_pending_row_prelude(buffer);
                }
            }
        }
    }

    fn emit_display_string_visual_wrap<B: LayoutBufferView>(
        &mut self,
        buffer: &B,
    ) -> DisplayRowTransitionContinuation {
        let continuation = emit_nested_source_visual_wrap(self.loop_context, self.state.reborrow());
        if !continuation.should_break() {
            self.render_pending_row_prelude(buffer);
        }
        continuation
    }

    fn render_pending_row_prelude<B: LayoutBufferView>(&mut self, buffer: &B) {
        if let Some(context) = self.row_prelude_context {
            self.state
                .render_row_prelude(context, self.params, self.active_face_state, buffer);
        }
    }

    /// A `display` string that ended in a newline terminated the current row;
    /// emit that row break so the buffer text after the covered region (which
    /// may be a bare newline that must still produce its own blank row) starts
    /// on a fresh row. Returns `false` when the break exhausted the window and
    /// the buffer walk must stop. GNU: xdisp.c `display_line` ends a display
    /// line on a display-string '\n'.
    fn emit_display_string_row_break<B: LayoutBufferView>(
        &mut self,
        source_walk: &mut BufferSourceWalk<'request, B>,
        buffer: &B,
        line_break: crate::display_row::replacement::DisplayReplacementStringLineBreak,
    ) -> bool
    where
        'surface: 'request,
    {
        let synthetic_newline = DisplaySourceStepChar::new(
            '\n',
            self.state.progress.byte_idx(),
            self.state.progress.charpos(),
        );
        !self
            .loop_context
            .line_break_request(
                synthetic_newline,
                self.text,
                self.state.surface.append_surface,
                self.active_face_state,
            )
            .with_display_string_line_break(line_break)
            .render_display_string_break_and_apply(source_walk, buffer, self.state.reborrow())
            .should_break()
    }

    fn render_source_item<B: LayoutBufferView>(
        &mut self,
        source_walk: &mut BufferSourceWalk<'request, B>,
        layout_resolution_context: BufferSourceItemLayoutResolutionContext<'request>,
        source_item: DisplaySourceStepItem,
        buffer: &B,
    ) -> bool
    where
        'surface: 'request,
    {
        // A `SourceMappedText` standing in for a display-table entry that ends in
        // a newline glyph (whitespace-mode `[$ \n]`) renders its leading glyphs
        // and then ends the row — GNU treats the trailing `\n` element as its own
        // end-of-line display element. The buffer newline is already consumed by
        // this item's span, so break WITHOUT consuming another char.
        let break_after_row = source_item.item().layout.break_after_row;
        let line_break = break_after_row.then(|| {
            let item = source_item.item();
            let face_id = render_face_ref_id(item.face, self.active_face_state.face_id());
            let face = source_walk
                .resolved_source_face(face_id)
                .unwrap_or_else(|| self.active_face_state.resolved_face());
            crate::display_row::replacement::DisplayReplacementStringLineBreak::from_resolved_face(
                face_id,
                face,
                self.active_face_state.metrics(),
                crate::display_item::DisplayLineHeightPolicy::Default,
                crate::display_item::DisplayLineSpacingPolicy::Inherit,
                item.box_vertical_edges,
            )
        });
        let keep_going = BufferSourceItemRenderRequest::from_loop_context(
            layout_resolution_context,
            self.loop_context,
            self.text,
            self.state.surface.append_surface,
            self.active_face_state,
            self.params,
        )
        .render_and_apply(
            source_item,
            source_walk,
            buffer,
            self.row_prelude_context,
            self.state.reborrow(),
        );
        if keep_going && let Some(line_break) = line_break {
            return self.emit_display_string_row_break(source_walk, buffer, line_break);
        }
        keep_going
    }
}
