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
use crate::display_item::DisplayItem;
use crate::display_row::builder::DisplayRowAppendStatus;
use crate::display_row::face_state::{DisplayRowActiveFaceState, DisplayRowExtendFace};
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_source::{DisplayItemSegmentSource, DisplaySourceStepItem};
use crate::display_source_append_plan::NaturalDisplayRowAppendRenderPolicy;
use crate::display_source_item_append::{
    DisplaySourcePreparedCharAppend, DisplaySourceTextCharPreparedAppend,
};
use crate::neovm_bridge::LayoutBufferView;
use crate::types::WindowParams;

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
pub(crate) fn render_source_char_and_apply<B: LayoutBufferView>(
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
    mut state: BufferSourceLoopMutableState<'_, '_, '_>,
) -> BufferSourceItemRenderOutcome {
    let text_start_byte = loop_context.text_start_byte();

    // Element granularity: this path renders exactly ONE character, so take
    // the run's first character and leave the rest to the producer. The walk
    // position advances by that one character (the append does it), and the
    // next iteration reads the remainder straight from the cursor — the
    // producer's position IS the resume state. The remainder used to be
    // split into N single-character items and pushed back through a pending
    // queue for later iterations to pop.
    if let Some(first) = source_item.clone().first_text_run_char(text_start_byte) {
        source_item = first;
    }

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
    source_step_char.record_word_wrap_candidate_at(
        state.row_carryover.word_wrap,
        &state.source_render,
        append_position,
        predecessor_row_extend,
    );

    let buffer_source_char = source_step_char.source_char(params.nobreak_char_display);
    let display_table_vector = source_item.is_display_table_vector();
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
    let overflow_outcome = BufferSourceOverflowRenderRequest::new(
        &prepared_append,
        source_step_char,
        BufferSourceOverflowRenderContext::new(
            ch,
            state.surface.append_surface.right_edge(),
            state.surface.append_surface.right_edge_marker_column(),
            params.wrap_mode,
            *state.row_carryover.word_wrap,
            loop_context.row_visibility_limit(),
            loop_context.content_x(),
            loop_context.has_prefix(),
            loop_context.row_geometry_defaults(),
            loop_context.display_text_row_base(),
            loop_context.max_rows(),
            loop_context.row_limit(),
            active_face_state.metrics(),
            loop_context.frame_background(),
        ),
    )
    .render_if_needed_and_apply(source_walk, text, state.reborrow());
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
    if let Some(end_charpos) = source_end_charpos {
        state.progress.max_charpos(end_charpos);
    }
    if let Some(end_byte_idx) = source_end_byte_idx {
        state.progress.set_byte_idx(end_byte_idx);
    }

    BufferSourceItemRenderOutcome::Rendered
}

#[allow(clippy::too_many_arguments)]
fn render_display_table_vector_and_apply<B: LayoutBufferView>(
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
    mut state: BufferSourceLoopMutableState<'_, '_, '_>,
) -> BufferSourceItemRenderOutcome {
    let mut source = DisplayItemSegmentSource::new(source_item);
    let mut source_state = DisplayRowSourceState::frame_local();
    let mut render_policy = NaturalDisplayRowAppendRenderPolicy;
    let mut cursor_pending = state
        .cursor_info
        .should_capture_visible_glyph_at(state.progress.charpos(), loop_context.point_charpos());
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
                capture_cursor_info(
                    state.cursor_info,
                    prepared_append.cursor_info_for_main_char_with_slot_width(
                        active_face_state,
                        state.row_build.row_geometry.text_position(
                            first_slot.x_px(),
                            source_step_char.start_byte_idx(),
                            first_slot.col(),
                        ),
                        first_slot.width_px(),
                        false,
                    ),
                );
                cursor_pending = false;
            }
            first_glyph_pending = false;
        }

        match append_progress.status() {
            DisplayRowAppendStatus::Complete => {
                if cursor_pending {
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
                prepared_append.apply_rendered_progress_to_walk_state(
                    append_progress,
                    source_step_char.ch(),
                    state.row_build.row_geometry,
                    state.row_carryover.trailing_whitespace,
                    state.row_carryover.word_wrap,
                    &mut state.progress.reborrow(),
                );
                if let Some(end_charpos) = source_end_charpos {
                    state.progress.max_charpos(end_charpos);
                }
                if let Some(end_byte_idx) = source_end_byte_idx {
                    state.progress.set_byte_idx(end_byte_idx);
                }
                return BufferSourceItemRenderOutcome::Rendered;
            }
            DisplayRowAppendStatus::Clipped => {
                state.progress.apply_row_position(append_progress.end());
                if params.wrap_mode == crate::types::LineWrapMode::Truncate {
                    let overflow_outcome = BufferSourceOverflowRenderRequest::new(
                        prepared_append,
                        source_step_char,
                        BufferSourceOverflowRenderContext::new(
                            source_step_char.ch(),
                            state.surface.append_surface.right_edge(),
                            state.surface.append_surface.right_edge_marker_column(),
                            params.wrap_mode,
                            *state.row_carryover.word_wrap,
                            loop_context.row_visibility_limit(),
                            loop_context.content_x(),
                            loop_context.has_prefix(),
                            loop_context.row_geometry_defaults(),
                            loop_context.display_text_row_base(),
                            loop_context.max_rows(),
                            loop_context.row_limit(),
                            active_face_state.metrics(),
                            loop_context.frame_background(),
                        ),
                    )
                    .render_if_needed_and_apply(
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
                unreachable!("display-table vector rows are extracted before item rendering")
            }
        }
    }
}
