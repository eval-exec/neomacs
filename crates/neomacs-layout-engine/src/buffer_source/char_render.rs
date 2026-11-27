//! Single-character buffer source rendering.

use crate::buffer_source::item_append::BufferSourceRowAppendContext;
use crate::buffer_source::item_render::BufferSourceItemRenderOutcome;
use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::buffer_source::loop_state::BufferSourceLoopMutableState;
use crate::buffer_source::overflow::{
    BufferSourceOverflowRenderContext, BufferSourceOverflowRenderRequest,
    BufferSourceSpecialOverflowRenderContext, BufferSourceSpecialOverflowRenderRequest,
};
use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
use crate::buffer_source::walk::BufferSourceWalk;
use crate::display_cursor::{capture_cursor_info, update_cursor_info_for_main_char};
use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplaySourcePosition, DisplayTextComposition,
};
use crate::display_row::builder::DisplayRowAppendStatus;
use crate::display_row::face_state::{DisplayRowActiveFaceState, DisplayRowExtendFace};
use crate::display_row::metrics::DisplayRowMeasuredFaceMetrics;
use crate::display_row::render_policy::{DisplayRowRenderClipBehavior, DisplayRowRenderPolicy};
use crate::display_row::replacement::DisplayReplacementStringLineBreak;
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_row::transition::{
    DisplayRowLineBreakTransitionPlan, DisplayRowOverflowTransitionPlan,
    DisplayRowTextWindowEmitContext,
};
use crate::display_source::{
    DisplayItemSegmentSource, DisplaySourceStepChar, DisplaySourceStepItem,
    DisplayVectorTruncateSkip,
};
use crate::display_source_item_append::{
    DisplaySourcePreparedCharAppend, DisplaySourceTextCharPreparedAppend,
};
use crate::neovm_bridge::{LayoutBufferView, ResolvedFace};
use crate::types::{LayoutCharPos0, WindowParams};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::LispCharPos1;

/// Render exactly ONE character of the Renderable element arm.
///
/// P4.8(c): this used to be a `BufferSourceCharRenderRequest` struct that
/// re-packed fourteen values the loop context already owns, rebuilt for every
/// character once the run fell to char granularity. The window-invariant half
/// of that bundle IS `loop_context`, so the arm reads it directly and only the
/// genuinely per-call references are passed. The append surface is NOT among
/// them: the loop state already carries the same one the request used to copy.
///
/// P4.9: the loop state is no longer destructured here. It was, and the two
/// overflow requests below each had to re-pack all twenty fields by hand to
/// hand it on; `reborrow` already expresses that, so the field-by-field
/// rebuilds are gone.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_source_char_and_apply<
    B: LayoutBufferView,
    FS: crate::neovm_bridge::FaceServices,
>(
    loop_context: BufferSourceLoopRequestContext,
    text: &[u8],
    params: &WindowParams,
    mut source_item: DisplaySourceStepItem,
    source_walk: &mut BufferSourceWalk<'_, B>,
    buffer: &B,
    row_prelude_context: Option<BufferSourceRowPreludeRequestContext>,
    active_face_state: &DisplayRowActiveFaceState,
    append_context: &BufferSourceRowAppendContext<'_, '_, B>,
    predecessor_row_extend: Option<DisplayRowExtendFace>,
    mut state: BufferSourceLoopMutableState<'_, '_, '_, FS>,
) -> BufferSourceItemRenderOutcome {
    let text_start_byte = loop_context.text_start_byte();

    // Element granularity: this path renders exactly ONE character, so take
    // the run's first character and leave the rest to the producer. The walk
    // position advances by that one character (the append does it), and the
    // next iteration reads the remainder straight from the cursor — the
    // producer's position IS the resume state. The remainder used to be
    // split into N single-character items and pushed back through a pending
    // queue for later iterations to pop.
    source_item.retain_first_text_run_char(text_start_byte);

    // An ordinary independent buffer glyph is a stable iterator position.
    // Compositions, replacements, display vectors and zero-width elements
    // need their existing complete-element/source-position lifecycle.
    let ordinary_target = params.query_target.is_some_and(|target| {
        params.wrap_mode == crate::types::LineWrapMode::Truncate
            && !params.word_wrap
            && !state.row_carryover.word_wrap.is_enabled()
            && target.get() == source_item.source_step_char().start_charpos()
            && source_item.source_end_charpos() == Some(target.get().saturating_add(1))
    }) && matches!(
        source_item.item().span.start,
        DisplaySourcePosition::Buffer { .. }
    ) && matches!(
        source_item.item().span.end,
        DisplaySourcePosition::Buffer { .. }
    ) && matches!(&source_item.item().kind,
            DisplayItemKind::TextRun(run) if run.composition == DisplayTextComposition::Independent
                && run.text.chars().eq(std::iter::once(source_item.source_step_char().ch())));

    let (source_step_char, source_end_charpos, source_end_byte_idx, source_item) =
        source_item.into_render_parts();
    let ch = source_step_char.ch();

    // Overlay strings are no longer emitted here. Since P4.6 the PRODUCER
    // surfaces them as a typed element at the anchor, with insertion
    // semantics, and the loop-level arm in render.rs appends them before the
    // buffer character at the same position — the GNU handle_stop order this
    // call used to implement by probing every character.
    let append_position = state.progress.row_position();
    let append_geometry = *state.row_build.row_geometry;
    let display_table_vector = source_item.is_display_table_vector();
    let empty_display_vector = display_table_vector
        && matches!(&source_item.kind, DisplayItemKind::SourceMappedText(mapped) if mapped.text.is_empty());
    // GNU get_next_display_element skips an empty vector before display_line
    // can save a word-wrap candidate for it (xdisp.c:8491-8517).
    if !empty_display_vector {
        source_step_char.record_word_wrap_candidate_at(
            state.row_carryover.word_wrap,
            &state.source_render,
            append_position,
            predecessor_row_extend,
        );
    }

    let buffer_source_char = source_step_char.source_char(params.nobreak_char_display);
    let prepared_append = if display_table_vector {
        DisplaySourcePreparedCharAppend::Text(
            append_context.prepare_display_vector_for_current_text_row(
                append_geometry,
                source_walk.append_state(),
                &mut state.source_render,
                &buffer_source_char,
                text,
                source_step_char.start_byte_idx(),
                append_position,
                &source_item,
            ),
        )
    } else {
        append_context.prepare_source_item_for_current_text_row(
            append_geometry,
            source_walk.append_state(),
            &mut state.source_render,
            &buffer_source_char,
            text,
            source_step_char.start_byte_idx(),
            append_position,
            &source_item,
        )
    };

    let prepared_append = match prepared_append {
        DisplaySourcePreparedCharAppend::Special(special_prepared_append) => {
            let special_overflow_outcome = BufferSourceSpecialOverflowRenderRequest::new(
                &special_prepared_append,
                BufferSourceSpecialOverflowRenderContext::new(
                    text,
                    text_start_byte,
                    state.progress.row_progress().x(),
                    state.surface.append_surface.full_text_right_edge(),
                    params.wrap_mode,
                    loop_context.row_visibility_limit(),
                    loop_context.content_x(),
                    loop_context.has_prefix(),
                    loop_context.row_geometry_defaults(),
                    loop_context.display_text_row_base(),
                    loop_context.max_rows(),
                    loop_context.row_limit(),
                ),
            )
            .render_if_needed_and_apply(source_walk, buffer, state.reborrow());
            if special_overflow_outcome.should_break() {
                return BufferSourceItemRenderOutcome::Stop;
            }
            if special_overflow_outcome.should_continue_buffer_walk() {
                return BufferSourceItemRenderOutcome::ContinueBufferWalk;
            }

            let Some(special_outcome) = special_prepared_append.append_to_text_row(
                append_context,
                state.row_build.row_geometry,
                params,
                state.face_ids,
                &mut state.source_render.reborrow(),
            ) else {
                return BufferSourceItemRenderOutcome::Stop;
            };
            special_outcome.capture_cursor_info_for_main_char_if_point(
                state.cursor_info,
                state.row_build.row_geometry,
                state.face_ids,
                source_step_char.start_byte_idx(),
                state.progress.charpos(),
                loop_context.point_charpos(),
            );
            special_outcome.apply_rendered_special_char_to_walk_state(
                state.face_scan,
                state.row_carryover.word_wrap,
                &mut state.progress.reborrow(),
            );
            if let Some(end_byte_idx) = source_end_byte_idx {
                state.progress.set_byte_idx(end_byte_idx);
            }
            return BufferSourceItemRenderOutcome::ContinueBufferWalk;
        }
        DisplaySourcePreparedCharAppend::Text(prepared_append) => prepared_append,
    };

    if display_table_vector {
        return render_display_table_vector_and_apply(
            loop_context,
            params,
            source_step_char,
            source_end_charpos,
            source_end_byte_idx,
            source_item,
            source_walk,
            text,
            buffer,
            row_prelude_context,
            active_face_state,
            append_context,
            &prepared_append,
            state,
        );
    }

    prepared_append
        .update_cursor_info_for_main_char(state.cursor_info, source_step_char.start_byte_idx());
    let row_position = state.progress.row_position();
    if ordinary_target
        && row_position.x_px() >= state.surface.append_surface.right_edge()
        && row_position.x_px() < state.surface.append_surface.line_end_fill_right_edge()
    {
        // GNU tests a truncated target before producing its glyph
        // (xdisp.c:10267-10280). The reserved marker column is still a
        // visible iterator position; a presentation marker cannot certify
        // that source position because it does not extend the row's span.
        let cursor = prepared_append.cursor_info_for_main_char(
            active_face_state,
            state.row_build.row_geometry.text_position(
                row_position.x_px(),
                source_step_char.start_byte_idx(),
                row_position.col(),
            ),
            ch == '\t',
        );
        if let Some(width) = cursor.slot_width.filter(|width| *width > 0.0) {
            state
                .source_render
                .include_current_row_metrics(state.row_build.row_geometry);
            state
                .source_render
                .output_emitter()
                .push_text_display_point(
                    neovm_core::buffer::LispCharPos1::new(source_step_char.start_charpos() + 1),
                    row_position.x_px(),
                    state.row_build.row_geometry.y(),
                    width,
                    state.row_build.row_geometry.height(),
                    state.row_build.row_geometry.row(),
                    row_position.col(),
                );
            if state.cursor_info.should_capture_visible_glyph_at(
                state.progress.charpos(),
                loop_context.point_charpos(),
            ) {
                capture_cursor_info(state.cursor_info, cursor);
            }
            state
                .source_render
                .output_emitter()
                .mark_query_target_reached();
            return BufferSourceItemRenderOutcome::Stop;
        }
    }
    let overflow_outcome = BufferSourceOverflowRenderRequest::new(
        &prepared_append,
        source_step_char,
        BufferSourceOverflowRenderContext::new(
            ch,
            state.surface.append_surface.right_edge(),
            state.surface.append_surface.right_edge_marker_column(),
            params.wrap_mode,
            state.row_carryover.word_wrap.checkpoint(),
            loop_context.row_visibility_limit(),
            loop_context.content_x(),
            loop_context.has_prefix(),
            loop_context.row_geometry_defaults(),
            loop_context.display_text_row_base(),
            loop_context.max_rows(),
            loop_context.row_limit(),
            active_face_state.metrics(),
            loop_context.frame_background(),
            loop_context.point_charpos(),
        ),
    )
    .render_if_needed_and_apply(append_context, source_walk, text, state.reborrow());
    if overflow_outcome.should_break() {
        return BufferSourceItemRenderOutcome::Stop;
    }
    if overflow_outcome.should_continue_buffer_walk() {
        return BufferSourceItemRenderOutcome::ContinueBufferWalk;
    }

    let row_position = state.progress.row_position();
    prepared_append.capture_cursor_info_for_main_char_if_point(
        state.cursor_info,
        active_face_state,
        state.row_build.row_geometry,
        row_position.x_px(),
        source_step_char.start_byte_idx(),
        row_position.col(),
        ch == '\t',
        state.progress.charpos(),
        loop_context.point_charpos(),
    );
    if state.cursor_info.is_missing()
        && source_end_charpos.is_some_and(|end| {
            loop_context.point_charpos() > state.progress.charpos()
                && loop_context.point_charpos() < end
        })
    {
        capture_cursor_info(
            state.cursor_info,
            prepared_append.cursor_info_for_main_char(
                active_face_state,
                state.row_build.row_geometry.text_position(
                    row_position.x_px(),
                    source_step_char.start_byte_idx(),
                    row_position.col(),
                ),
                ch == '\t',
            ),
        );
    }

    if prepared_append
        .append_to_text_row_and_apply(
            append_context,
            &append_geometry,
            ch,
            &mut state.source_render.reborrow(),
            state.row_carryover.trailing_whitespace,
            state.row_carryover.word_wrap,
            &mut state.progress.reborrow(),
        )
        .should_break()
    {
        return BufferSourceItemRenderOutcome::Stop;
    }
    state
        .source_render
        .include_current_row_metrics(state.row_build.row_geometry);
    if let Some(end_charpos) = source_end_charpos {
        state.progress.max_charpos(end_charpos);
    }
    if let Some(end_byte_idx) = source_end_byte_idx {
        state.progress.set_byte_idx(end_byte_idx);
    }

    if ordinary_target && state.progress.row_position().x_px() > row_position.x_px() {
        state
            .source_render
            .output_emitter()
            .mark_query_target_reached();
        return BufferSourceItemRenderOutcome::Stop;
    }

    BufferSourceItemRenderOutcome::Rendered
}

struct DisplayVectorRenderPolicy {
    fallback_metrics: DisplayRowMeasuredFaceMetrics,
    produced_row_break: Option<DisplayReplacementStringLineBreak>,
}

impl DisplayVectorRenderPolicy {
    fn new(fallback_metrics: DisplayRowMeasuredFaceMetrics) -> Self {
        Self {
            fallback_metrics,
            produced_row_break: None,
        }
    }
}

impl DisplayRowRenderPolicy for DisplayVectorRenderPolicy {
    fn stop_before_item(
        &mut self,
        item: &DisplayItem,
        face_id: FaceId,
        face: &ResolvedFace,
        newline_font_metrics: Option<DisplayRowMeasuredFaceMetrics>,
    ) -> bool {
        if let DisplayItemKind::RowBreak(row_break) = item.kind {
            let mut line_break = DisplayReplacementStringLineBreak::from_resolved_face(
                face_id,
                face,
                self.fallback_metrics,
                row_break.line_height,
                row_break.line_spacing,
                item.box_vertical_edges,
            );
            if let Some(metrics) = newline_font_metrics {
                line_break = line_break.with_measured_metrics(metrics, face_id);
            }
            self.produced_row_break = Some(line_break);
            true
        } else {
            false
        }
    }

    fn clipped_behavior(&mut self, item: &DisplayItem) -> DisplayRowRenderClipBehavior {
        if matches!(item.kind, DisplayItemKind::SourceMappedText(_)) {
            DisplayRowRenderClipBehavior::PreserveRemainderAndStop
        } else {
            DisplayRowRenderClipBehavior::Continue
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_display_vector_row_break<B: LayoutBufferView, FS: crate::neovm_bridge::FaceServices>(
    loop_context: BufferSourceLoopRequestContext,
    text: &[u8],
    active_face_state: &DisplayRowActiveFaceState,
    source_walk: &mut BufferSourceWalk<'_, B>,
    buffer: &B,
    line_break: DisplayReplacementStringLineBreak,
    transition: DisplayRowLineBreakTransitionPlan,
    mut state: BufferSourceLoopMutableState<'_, '_, '_, FS>,
) -> bool {
    let synthetic_newline =
        DisplaySourceStepChar::new('\n', state.progress.byte_idx(), state.progress.charpos());
    !loop_context
        .line_break_request(
            synthetic_newline,
            text,
            state.surface.append_surface,
            active_face_state,
        )
        .with_display_string_line_break(line_break)
        .render_display_string_break_and_apply(source_walk, buffer, state.reborrow(), transition)
        .should_break()
}

#[allow(clippy::too_many_arguments)]
fn emit_display_vector_truncated_row_end<FS: crate::neovm_bridge::FaceServices>(
    loop_context: BufferSourceLoopRequestContext,
    source_step_char: DisplaySourceStepChar,
    active_face_state: &DisplayRowActiveFaceState,
    truncated_end_charpos: i64,
    crossed_buffer_newline: bool,
    mut state: BufferSourceLoopMutableState<'_, '_, '_, FS>,
) -> bool {
    if state
        .surface
        .append_surface
        .right_edge_marker_column()
        .is_reserved()
    {
        let marker_position = state.progress.row_position();
        let metrics = active_face_state.metrics();
        state
            .source_render
            .output_emitter()
            .push_text_overlaid_marker_point(
                LispCharPos1::new(source_step_char.start_charpos() + 1),
                marker_position.x_px(),
                state.row_build.row_geometry.y(),
                metrics.char_width(),
                state.row_build.row_geometry.height(),
                state.row_build.row_geometry.row(),
                marker_position.col(),
            );
    }
    state.progress.reset_physical_line_tabs();
    state
        .source_render
        .output_emitter()
        .note_truncated_end(LispCharPos1::new(truncated_end_charpos));
    if crossed_buffer_newline {
        state.row_carryover.line_numbers.advance_line();
    } else {
        state.row_carryover.line_numbers.mark_display_string_break();
    }
    *state.progress.row_progress_mut().x_mut() = loop_context.content_x();
    state.row_build.row_extend.clear();
    let row_position = state.progress.row_position();
    let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
        loop_context.row_geometry_defaults(),
        loop_context.display_text_row_base(),
        state.row_y_positions,
        loop_context.max_rows(),
        state.row_build.row_geometry,
        state.row_build.row_flags,
        loop_context.row_limit(),
        &mut state.source_render,
    )
    .emit_overflow_then_row_start(
        DisplayRowOverflowTransitionPlan::truncation(
            crate::display_row::walk_state::TextRowTransitionStatePolicy::truncation(),
        ),
        LayoutCharPos0::new(state.progress.charpos()),
        row_position,
        state.row_carryover.render_state(loop_context.has_prefix()),
        state.progress.row_progress_mut().col_mut(),
    );
    !row_transition.is_exhausted()
}

#[allow(clippy::too_many_arguments)]
fn render_display_table_vector_and_apply<
    B: LayoutBufferView,
    FS: crate::neovm_bridge::FaceServices,
>(
    loop_context: BufferSourceLoopRequestContext,
    params: &WindowParams,
    source_step_char: crate::display_source::DisplaySourceStepChar,
    source_end_charpos: Option<i64>,
    source_end_byte_idx: Option<usize>,
    source_item: DisplayItem,
    source_walk: &mut BufferSourceWalk<'_, B>,
    text: &[u8],
    buffer: &B,
    row_prelude_context: Option<BufferSourceRowPreludeRequestContext>,
    active_face_state: &DisplayRowActiveFaceState,
    append_context: &BufferSourceRowAppendContext<'_, '_, B>,
    prepared_append: &DisplaySourceTextCharPreparedAppend,
    mut state: BufferSourceLoopMutableState<'_, '_, '_, FS>,
) -> BufferSourceItemRenderOutcome {
    let empty_display_vector = matches!(&source_item.kind, DisplayItemKind::SourceMappedText(mapped) if mapped.text.is_empty());
    let mut source = DisplayItemSegmentSource::new(source_item);
    let mut source_state = DisplayRowSourceState::frame_local();
    let mut render_policy = DisplayVectorRenderPolicy::new(active_face_state.metrics());
    let mut cursor_pending = state
        .cursor_info
        .should_capture_visible_glyph_at(state.progress.charpos(), loop_context.point_charpos());
    let mut pending_row_cursor: Option<crate::display_cursor::CapturedCursorInfo> = None;
    let mut first_glyph_pending = true;
    loop {
        let position = state.progress.row_position();
        let Some(append_progress) = append_context.render_display_item_source_to_text_row(
            state.row_build.row_geometry,
            &mut state.source_render.reborrow(),
            &mut source,
            &mut source_state,
            position,
            crate::display_row::append_context::DisplayRowAppendKind::SourceText,
            &mut render_policy,
        ) else {
            return BufferSourceItemRenderOutcome::Stop;
        };

        if first_glyph_pending && let Some(first_slot) = append_progress.slots().first() {
            update_cursor_info_for_main_char(
                state.cursor_info,
                source_step_char.start_byte_idx(),
                first_slot.width_px(),
            );
            if cursor_pending {
                // Defer committing this row's cursor candidate until we know
                // whether this row completes the character or ends mid-vector
                // on a non-continued embedded `?\n` (GNU `row_for_charpos_p`,
                // `src/xdisp.c:25000-25014`: `MATRIX_ROW_ENDS_IN_MIDDLE_OF_CHAR_P`
                // only keeps the cursor on `row->continued_p`; otherwise the
                // subsequent row carrying the vector's suffix owns the cursor).
                pending_row_cursor =
                    Some(prepared_append.cursor_info_for_main_char_with_slot_width(
                        active_face_state,
                        state.row_build.row_geometry.text_position(
                            first_slot.x_px(),
                            source_step_char.start_byte_idx(),
                            first_slot.col(),
                        ),
                        first_slot.width_px(),
                        false,
                    ));
                cursor_pending = false;
            }
            first_glyph_pending = false;
        }

        if let Some(line_break) = render_policy.produced_row_break.take() {
            // A leading `?\n` in a display vector (e.g. `[?\n ?X]`) has no
            // preceding glyph on this row; publish its display point at the
            // row-break column so `posn-at-point` matches GNU's `move_it_to`
            // (row 0 col 1 for `"A\nB\n"` at point 2).
            let end_pos = append_progress.end();
            if first_glyph_pending {
                let metrics = active_face_state.metrics();
                state
                    .source_render
                    .output_emitter()
                    .push_text_display_point(
                        LispCharPos1::new(source_step_char.start_charpos() + 1),
                        end_pos.x_px(),
                        state.row_build.row_geometry.y(),
                        metrics.char_width(),
                        state.row_build.row_geometry.height(),
                        state.row_build.row_geometry.row(),
                        end_pos.col(),
                    );
                if cursor_pending {
                    pending_row_cursor =
                        Some(prepared_append.cursor_info_for_main_char_with_slot_width(
                            active_face_state,
                            state.row_build.row_geometry.text_position(
                                end_pos.x_px(),
                                source_step_char.start_byte_idx(),
                                end_pos.col(),
                            ),
                            metrics.char_width(),
                            false,
                        ));
                }
            }
            state.progress.apply_row_position(end_pos);
            if source.has_remaining() {
                // Non-continued row ending mid-display-vector: GNU
                // `row_for_charpos_p` (`src/xdisp.c:25000-25014`) returns
                // `row->continued_p` (false), so drop `pending_row_cursor`
                // and let the row completing the vector capture it.
                let next_dpvec_index = source.next_dpvec_index();
                state.progress.set_dpvec_index(next_dpvec_index);
                let keep_going = emit_display_vector_row_break(
                    loop_context,
                    text,
                    active_face_state,
                    source_walk,
                    buffer,
                    line_break,
                    DisplayRowLineBreakTransitionPlan::display_string_line_break(),
                    state.reborrow(),
                );
                if !keep_going {
                    return BufferSourceItemRenderOutcome::Stop;
                }
                state.progress.set_dpvec_index(next_dpvec_index);
                return BufferSourceItemRenderOutcome::ContinueBufferWalk;
            }
            if let Some(cursor) = pending_row_cursor.take() {
                capture_cursor_info(state.cursor_info, cursor);
            }
            if let Some(end_charpos) = source_end_charpos {
                state.progress.max_charpos(end_charpos);
            }
            if let Some(end_byte_idx) = source_end_byte_idx {
                state.progress.set_byte_idx(end_byte_idx);
            }
            let transition = if source_step_char.ch() == '\n' {
                DisplayRowLineBreakTransitionPlan::line_break()
            } else {
                DisplayRowLineBreakTransitionPlan::display_string_line_break()
            };
            let keep_going = emit_display_vector_row_break(
                loop_context,
                text,
                active_face_state,
                source_walk,
                buffer,
                line_break,
                transition,
                state.reborrow(),
            );
            return if keep_going {
                BufferSourceItemRenderOutcome::ContinueBufferWalk
            } else {
                BufferSourceItemRenderOutcome::Stop
            };
        }

        match append_progress.status() {
            DisplayRowAppendStatus::Complete => {
                if let Some(cursor) = pending_row_cursor.take() {
                    capture_cursor_info(state.cursor_info, cursor);
                } else if cursor_pending {
                    state.cursor_info.defer_zero_width_to_next_glyph(
                        prepared_append.cursor_info_for_main_char_with_slot_width(
                            active_face_state,
                            state.row_build.row_geometry.text_position(
                                append_progress.start().x_px(),
                                source_step_char.start_byte_idx(),
                                append_progress.start().col(),
                            ),
                            active_face_state.metrics().char_width(),
                            false,
                        ),
                    );
                }
                if empty_display_vector {
                    state.progress.advance_charpos_by_one();
                } else {
                    prepared_append.apply_rendered_progress_to_walk_state(
                        append_progress,
                        source_step_char.ch(),
                        state.row_build.row_geometry,
                        state.row_carryover.trailing_whitespace,
                        state.row_carryover.word_wrap,
                        &mut state.progress.reborrow(),
                    );
                }
                if let Some(end_charpos) = source_end_charpos {
                    state.progress.max_charpos(end_charpos);
                }
                if let Some(end_byte_idx) = source_end_byte_idx {
                    state.progress.set_byte_idx(end_byte_idx);
                }
                if source_step_char.ch() == '\n' {
                    state.row_carryover.line_numbers.resume_after_replacement(1);
                }
                return BufferSourceItemRenderOutcome::Rendered;
            }
            DisplayRowAppendStatus::Clipped => {
                state.progress.apply_row_position(append_progress.end());
                if params.wrap_mode == crate::types::LineWrapMode::Truncate {
                    match source.skip_to_next_row_break() {
                        DisplayVectorTruncateSkip::ResumeAfterEmbeddedNewline {
                            next_dpvec_index,
                        } => {
                            // Truncated row ends mid-vector before an embedded
                            // `?\n`: `continued_p` is false so the suffix row
                            // owns the cursor (GNU `row_for_charpos_p`).
                            state.progress.set_dpvec_index(next_dpvec_index);
                            let keep_going = emit_display_vector_truncated_row_end(
                                loop_context,
                                source_step_char,
                                active_face_state,
                                source_step_char.start_charpos() + 1,
                                false,
                                state.reborrow(),
                            );
                            if !keep_going {
                                return BufferSourceItemRenderOutcome::Stop;
                            }
                            state.progress.set_dpvec_index(next_dpvec_index);
                            return BufferSourceItemRenderOutcome::ContinueBufferWalk;
                        }
                        DisplayVectorTruncateSkip::ConsumedTrailingNewline => {
                            if let Some(cursor) = pending_row_cursor.take() {
                                capture_cursor_info(state.cursor_info, cursor);
                            }
                            let end_charpos = source_end_charpos
                                .unwrap_or_else(|| source_step_char.start_charpos() + 1);
                            if let Some(end_charpos) = source_end_charpos {
                                state.progress.max_charpos(end_charpos);
                            }
                            if let Some(end_byte_idx) = source_end_byte_idx {
                                state.progress.set_byte_idx(end_byte_idx);
                            }
                            let keep_going = emit_display_vector_truncated_row_end(
                                loop_context,
                                source_step_char,
                                active_face_state,
                                end_charpos,
                                source_step_char.ch() == '\n',
                                state.reborrow(),
                            );
                            return if keep_going {
                                BufferSourceItemRenderOutcome::ContinueBufferWalk
                            } else {
                                BufferSourceItemRenderOutcome::Stop
                            };
                        }
                        DisplayVectorTruncateSkip::NoNewlineInVector => {
                            if let Some(cursor) = pending_row_cursor.take() {
                                capture_cursor_info(state.cursor_info, cursor);
                            }
                        }
                    }
                    let overflow_outcome = BufferSourceOverflowRenderRequest::new(
                        &prepared_append,
                        source_step_char,
                        BufferSourceOverflowRenderContext::new(
                            source_step_char.ch(),
                            state.surface.append_surface.right_edge(),
                            state.surface.append_surface.right_edge_marker_column(),
                            params.wrap_mode,
                            state.row_carryover.word_wrap.checkpoint(),
                            loop_context.row_visibility_limit(),
                            loop_context.content_x(),
                            loop_context.has_prefix(),
                            loop_context.row_geometry_defaults(),
                            loop_context.display_text_row_base(),
                            loop_context.max_rows(),
                            loop_context.row_limit(),
                            active_face_state.metrics(),
                            loop_context.frame_background(),
                            loop_context.point_charpos(),
                        ),
                    )
                    .render_if_needed_and_apply(
                        append_context,
                        source_walk,
                        text,
                        state.reborrow(),
                    );
                    if overflow_outcome.should_continue_buffer_walk() {
                        return BufferSourceItemRenderOutcome::ContinueBufferWalk;
                    }
                    debug_assert!(overflow_outcome.should_break());
                    return BufferSourceItemRenderOutcome::Stop;
                }
                if let Some(cursor) = pending_row_cursor.take() {
                    // Wrapped row (`continued_p == true`) retains the cursor
                    // even when ending mid-vector (GNU `row_for_charpos_p`).
                    capture_cursor_info(state.cursor_info, cursor);
                }
                let continuation = crate::buffer_source::render::emit_nested_source_visual_wrap(
                    loop_context,
                    state.reborrow(),
                );
                if continuation.should_break() {
                    return BufferSourceItemRenderOutcome::Stop;
                }
                if let Some(row_prelude_context) = row_prelude_context {
                    state.render_row_prelude(
                        row_prelude_context,
                        params,
                        active_face_state,
                        buffer,
                    );
                }
            }
            DisplayRowAppendStatus::RowBreak => {
                unreachable!("display-table vector RowBreak items are handled via stop_before_item")
            }
        }
    }
}
