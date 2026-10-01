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
use crate::buffer_source::overflow::BufferSourceWordWrapAction;
use crate::buffer_source::producer::ProducedStep;
use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
use crate::buffer_source::text_source::BufferOverlayStringsItem;
use crate::buffer_source::walk::{BufferSourceRewind, BufferSourceWalk};
use crate::coords::layout_i64_char_pos_to_lisp_char_pos;
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
use crate::display_row::walk_state::{TextRowTransitionStatePolicy, WordWrapBreakCandidate};
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

/// GNU `display_line`'s `back_to_wrap`: roll the row back to the word-wrap
/// break candidate and continue the walk from there.
///
/// A display element that does not fit a continued row is not moved down on its
/// own while the row holds a break candidate:
///
/// ```c
/// 		  else if (wrap_row_used > 0)
/// 		    {
/// 		    back_to_wrap:
/// 		      ...
/// 		      row->used[TEXT_AREA] = wrap_row_used;
/// 		      ...
/// 		      row->continued_p = true;
/// 		      ...
/// 		      RESTORE_IT (it, &wrap_it, wrap_data);
/// 		      it->continuation_lines_width += wrap_x;
/// 		    }
/// ```
///
/// (src/xdisp.c:26379-26406, emacs-31.1) -- the arm that wins over "Restore
/// positions to values before the element" at :26433-26475, so the word in
/// front of the element moves down WITH it instead of the element moving alone.
/// `wrap_row_used` is only ever saved under WORD_WRAP (:26091-26116), which is
/// why the caller tests the row's resolved `it->line_wrap`.
///
/// The rollback is the same one a word-wrapping TEXT character performs
/// (`BufferSourceWordWrapAction` in `buffer_source::overflow`): restore the
/// row's drawn glyphs, its display points, its `:extend` state, the pen and the
/// source producer to the candidate, then end the row as continued.  The
/// candidate character is re-produced on the continuation row by the ordinary
/// walk, which re-produces everything between it and the refused element too.
///
/// Measured, GNU Emacs 31.1 under Xvfb, 720 px text area, 9 px column, 40
/// columns of text then " zzz" and a 400 px image: with `word-wrap` nil the
/// first screen line is [1..47) -- all of the text plus the image, cropped to
/// 720 -- and with `word-wrap` t it is [1..42), ending BEFORE the word, with
/// [42..47) -- "zzz" and the whole 400 px image -- on the next line
/// (`tmp/midrow-image/case-3.txt`, `case-1.txt`).
fn emit_word_wrap_rewind<B: LayoutBufferView>(
    loop_context: BufferSourceLoopRequestContext,
    state: BufferSourceLoopMutableState<'_, '_, '_>,
    source_walk: &mut BufferSourceWalk<'_, B>,
    break_candidate: WordWrapBreakCandidate,
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
    let action = BufferSourceWordWrapAction::new(break_candidate);
    let right_edge_px = surface.append_surface.right_edge();
    progress.continue_physical_line_after_visual_row(
        action.row_position().x_px(),
        loop_context.content_x(),
    );
    source_render.restore_word_wrap_checkpoint(break_candidate);
    action.restore_row_extend(row_build.row_extend, row_build.row_geometry);
    {
        let box_vertical_edges = source_render.trailing_box_run_terminal();
        source_render.extend_face_to_end_of_line(
            row_build.row_extend,
            row_build.row_geometry,
            action.row_position().x_px(),
            right_edge_px,
            loop_context.frame_background(),
            box_vertical_edges,
        );
    }
    let mut source_position = progress.source_position();
    {
        let (x, col) = progress.row_progress_mut().coordinates_mut();
        action.apply_before_row_transition(
            source_render.output_emitter(),
            &mut source_position,
            col,
            row_build.row_extend,
            x,
            loop_context.content_x(),
        );
    }
    source_walk.rewind_source_consumption(BufferSourceRewind::WordWrap(source_position));
    source_walk
        .source_position_update(source_position)
        .apply_to_progress(&mut progress);

    let transition = DisplayRowOverflowTransitionPlan::visual_wrap(
        VisualWrapBreak::AtWordBoundary,
        TextRowTransitionStatePolicy::visual_wrap(),
    );
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
    .emit_overflow(
        transition,
        LayoutCharPos0::new(progress.charpos()),
        progress.row_position(),
    );
    let continuation = action.apply_after_row_transition_and_prefix(
        row_transition,
        transition,
        &mut source_position,
        row_source_start,
        face_scan,
        row_build.row_geometry,
        loop_context.row_visibility_limit(),
        row_carryover.render_state(loop_context.has_prefix()),
    );
    // GNU `maybe_produce_line_number`: a wrapped continuation row reserves a
    // blank (no-number) line-number gutter so its text aligns with the first
    // row's text column.
    row_carryover.line_numbers.mark_continuation_row();
    source_walk
        .source_position_update(source_position)
        .apply_to_progress(&mut progress);
    continuation
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
        let mut produced = ProducedStep::empty(self.state.progress.source_position());
        source_walk.consume_source_item_for_render_into(
            &mut self.state.progress,
            face_resolution_context,
            self.state.face_ids,
            &mut self.state.source_render.reborrow(),
            self.state.row_build.row_geometry,
            self.state.surface.append_surface,
            self.active_face_state,
            &mut produced,
        );
        let Some(consumed_item) = produced.source_item else {
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

    /// Render one `display` replacement, moving a replacement the row refuses
    /// down to the next row.
    ///
    /// GNU `display_line` does not drop a display element that draws past the
    /// right edge of a *continued* row: it removes it from the row, restores the
    /// iterator to before it, marks the row continued, and produces the element
    /// again at the start of the next row (`display_line`,
    /// src/xdisp.c:26448-26475).  That is also where `produce_image_glyph`'s
    /// wide-glyph crop does not apply, because the image starts at column zero
    /// on the new row (:32492-32509), so a mid-row image that does not fit is
    /// displayed below rather than cropped or lost.
    ///
    /// The move happens at most once per replacement: GNU's own guarantee is
    /// that a glyph at `hpos == 0` is kept (it crops to the row instead of
    /// refusing), so a replacement the *fresh* row also refuses -- one with no
    /// room at all, the degenerate geometry this port documents on
    /// `DisplayImageOverflowAction` -- takes the row's own answer instead of
    /// being deferred for ever.  A truncating row does not move the
    /// replacement down either: GNU draws that glyph past the edge.
    ///
    /// GNU prefers an earlier break: when the row has a word-wrap break
    /// candidate, it rewinds to that candidate first and the element moves down
    /// together with the word in front of it (`else if (wrap_row_used > 0) goto
    /// back_to_wrap`, src/xdisp.c:26379-26406).  Without a candidate it takes
    /// the arm below, which moves the element alone.  Both are measured on GNU
    /// Emacs 31.1 in `emit_word_wrap_rewind`'s note and in
    /// `tmp/midrow-image/wrap-modes.txt`.
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
        let mut moved_to_next_row = false;
        loop {
            // GNU removes the element and re-produces it on the next row only
            // while the row can continue; a truncating row draws past its edge
            // instead, and a replacement a *fresh* row also refuses (a row with
            // no room at all) has nowhere left to go.
            //
            // The row's resolved `it->line_wrap` answers "can it continue",
            // not `LineWrapMode`: only GNU's two wrapping methods reach the
            // element-move arms at all.
            let move_to_next_row =
                !moved_to_next_row && self.state.surface.append_surface.line_wrap().wraps();
            // GNU's pen runs in the shared coordinate space that INCLUDES the
            // hscrolled columns: `it->current_x` starts the row at
            // `it->first_visible_x` (src/xdisp.c:3500). The row pen here is
            // screen-relative, so a stretch resolved against it pinned
            // `:align-to` fields to screen columns while the text around them
            // scrolled (issue #446). The consumed hscroll columns are the
            // difference between the two spaces.
            let hscroll_offset_px = self.state.row_carryover.hscroll_skip.consumed_columns() as f32
                * self.params.char_width.max(1.0);
            let replacement_context = BufferDisplayPropertyTextReplacementRenderContext::new(
                replacement.clone(),
                self.loop_context.text_start_byte(),
                self.text,
                self.loop_context.content_x(),
                self.params,
                0.0,
                self.loop_context.char_height(),
                self.active_face_state,
                self.state.progress.row_progress().x() + hscroll_offset_px,
                self.state.progress.row_position(),
            );
            let rendered_state = BufferDisplayPropertyTextReplacementRenderState::new(
                self.state.source_render.reborrow(),
                self.state.face_ids,
                self.state.surface.append_surface,
                self.state.row_build.row_geometry,
                self.active_face_state,
            );
            let outcome = if move_to_next_row {
                replacement_context.render_and_report_refusal(
                    buffer,
                    rendered_state,
                    &mut self.state.progress,
                    self.state.cursor_info,
                    self.loop_context.point_charpos(),
                )
            } else {
                replacement_context.render_and_apply(
                    buffer,
                    rendered_state,
                    &mut self.state.progress,
                    self.state.cursor_info,
                    self.loop_context.point_charpos(),
                )
            };
            match outcome {
                BufferDisplayPropertyTextReplacementApplyOutcome::Applied => return true,
                BufferDisplayPropertyTextReplacementApplyOutcome::RefusedWhole => {
                    // `wrap_row_used > 0`: a break candidate stands earlier on
                    // this row, so the word in front of the element moves down
                    // with it.  The refused replacement drew no glyph, so the
                    // candidate is still the row's own and the rollback is
                    // exactly the one a wrapping text character performs.
                    if let Some(break_candidate) = self.word_wrap_break_before_replacement() {
                        let continuation = emit_word_wrap_rewind(
                            self.loop_context,
                            self.state.reborrow(),
                            source_walk,
                            break_candidate,
                        );
                        if continuation.should_break() {
                            return false;
                        }
                        // The continuation row re-produces the candidate and
                        // everything up to this replacement; the walk resumes
                        // there, in this call, with the state the rewind left.
                        return true;
                    }
                    moved_to_next_row = true;
                    // The refused replacement consumed no buffer text and drew
                    // no glyph, so the row ends exactly where it began and the
                    // walk position still names the covered character.
                    if self.emit_replacement_visual_wrap(buffer).should_break() {
                        return false;
                    }
                }
                BufferDisplayPropertyTextReplacementApplyOutcome::String(session) => {
                    return self.render_display_string_session(
                        source_walk,
                        buffer,
                        &replacement_context,
                        session,
                    );
                }
                BufferDisplayPropertyTextReplacementApplyOutcome::Fallback(source_item) => {
                    return self.render_source_item(
                        source_walk,
                        layout_resolution_context,
                        source_item,
                        buffer,
                    );
                }
                BufferDisplayPropertyTextReplacementApplyOutcome::Stop => return false,
            }
        }
    }

    /// The row's word-wrap break candidate, when rewinding to it would put the
    /// refused replacement further along the continuation row than it is now.
    ///
    /// `None` for a row that cannot word-wrap, for a row without a candidate,
    /// and for a candidate at or after the pen -- `wrap_row_used > 0` is
    /// strictly "something was drawn before the break point", and rewinding to
    /// a point the row has not passed would rewind nothing and loop.
    fn word_wrap_break_before_replacement(&self) -> Option<WordWrapBreakCandidate> {
        if !self.state.surface.append_surface.line_wrap().is_word_wrap() {
            return None;
        }
        let candidate = self.state.row_carryover.word_wrap.candidate();
        (self.state.row_carryover.word_wrap.has_candidate()
            && candidate.row_position().x_px() < self.state.progress.row_progress().x())
        .then_some(candidate)
    }

    /// End the current row for a replacement that has to move down, and start
    /// the continuation row the retry renders into.
    ///
    /// A visual row break, not a display-string one: the replacement belongs to
    /// a buffer character, so the new row starts at that character and the
    /// row's end source stays `Buffer`.
    fn emit_replacement_visual_wrap<B: LayoutBufferView>(
        &mut self,
        buffer: &B,
    ) -> DisplayRowTransitionContinuation {
        let continuation = emit_nested_source_visual_wrap(self.loop_context, self.state.reborrow());
        if !continuation.should_break() {
            self.render_pending_row_prelude(buffer);
        }
        continuation
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
        // Wrapping a replacement, like its explicit newline, ends a visual
        // row without consuming the buffer character that owns the string.
        let anchor = layout_i64_char_pos_to_lisp_char_pos(self.state.progress.charpos());
        self.state
            .source_render
            .output_emitter()
            .note_display_string_wrap(anchor);
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
