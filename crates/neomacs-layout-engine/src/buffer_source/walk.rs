//! Buffer text source walking and source-position updates.
//!
//! The main buffer renderer owns orchestration, while this module owns
//! source cursor driving, pending face installation, and source-position
//! updates used by row lifecycle renderers.

use crate::buffer_source::consumption::BufferSourceConsumedItem;
use crate::buffer_source::face_resolution::BufferSourceFaceResolutionContext;
use crate::buffer_source::overflow::BufferSourceTruncationSkipAction;
use crate::buffer_source::producer::{BufferElementProducer, ProducedStep};
use crate::buffer_source::row_lifecycle::{
    BufferSourceHscrollSkipAction, BufferSourceInvisibleTextScanAction,
    BufferSourceInvisibleTextScanContext, BufferSourceSelectiveDisplayContext,
    BufferSourceSelectiveDisplayHiddenLines, BufferSourceSelectiveDisplayLineTailAction,
    consume_hscroll_skip_from_position,
};
use crate::display_row::append_context::{
    DisplayRowActiveFaceAppendContext, DisplayRowAppendSurface,
};
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::DisplayRowGeometryState;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::walk_state::{
    HorizontalScrollSkipState, InvisibleTextScanCheckpoint, LineNumberRenderState,
};
use crate::display_source::DisplaySourceTextPosition;
use crate::display_source_item_append::DisplaySourceRowAppendState;
use crate::display_source_progress::DisplaySourceProgressState;
use crate::display_source_walk::DisplaySourcePositionConsumption;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{LayoutBufferView, ResolvedFace};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::BufferId;

pub(crate) struct BufferSourceWalk<'request, B: LayoutBufferView> {
    producer: BufferElementProducer<'request, B>,
    append_state: DisplaySourceRowAppendState,
}

/// Why the buffer producer is being reseated at an overflowing character.
///
/// Word wrap restores a checkpoint taken before every source element at the
/// boundary, so insertion elements must replay. Character wrap retries only
/// the overflowing buffer character and must preserve already-drawn insertion
/// elements. Keeping the distinction closed prevents a new retry call site
/// from silently choosing the wrong overlay-string behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceRewind {
    WordWrap(DisplaySourceTextPosition),
    CharacterWrap(DisplaySourceTextPosition),
}

/// Apply a produced step's side effects to the row being assembled: publish the
/// walk position when no element was produced, install the faces the resolver
/// collected, and record `(left-fringe …)` / `(right-fringe …)` specs.
fn apply_produced_step_to_progress(
    step: ProducedStep,
    progress: &mut DisplaySourceProgressState<'_>,
) -> (
    Option<BufferSourceConsumedItem>,
    Vec<crate::display_source_resolver::PendingDisplaySourceFace>,
    Vec<crate::display_source::DisplayNonTextAreaEmission>,
) {
    let ProducedStep {
        source_item,
        source_position,
        pending_faces,
        pending_non_text_area,
    } = step;
    if source_item.is_none() {
        progress.apply_source_position(source_position);
    }
    (source_item, pending_faces, pending_non_text_area)
}

fn apply_produced_step_to_render_progress<B: LayoutBufferView>(
    step: ProducedStep,
    progress: &mut DisplaySourceProgressState<'_>,
    face_resolution_context: BufferSourceFaceResolutionContext<'_, B>,
    source_render: &mut TextRowSourceRenderState<'_>,
    row_geometry: &mut DisplayRowGeometryState,
    append_surface: &DisplayRowAppendSurface,
    active_face_state: &DisplayRowActiveFaceState,
    face_ids: &mut FrameFaceAttempt,
) -> Option<BufferSourceConsumedItem> {
    let (source_item, pending_faces, pending_non_text_area) =
        apply_produced_step_to_progress(step, progress);
    face_resolution_context.install_pending_source_faces(
        source_render,
        row_geometry,
        pending_faces,
    );
    let fallback_metrics =
        DisplayRowFallbackMetrics::from_measured_face(active_face_state.metrics());
    let frame = DisplayRowActiveFaceAppendContext::new(
        append_surface,
        row_geometry,
        active_face_state,
        0.0,
        fallback_metrics,
    )
    .active_face_frame();
    for emission in pending_non_text_area {
        let structural_order = if progress.row_position().col() == 0 {
            crate::display_row::source_render::DisplayStructuralAreaOrder::BeforeExisting
        } else {
            crate::display_row::source_render::DisplayStructuralAreaOrder::AfterExisting
        };
        source_render.render_non_text_area_emission(
            emission,
            crate::display_source_resolver::DisplaySourceFaceScope::for_buffer(
                face_resolution_context.buffer(),
            ),
            &frame,
            face_ids,
            active_face_state.face_id(),
            structural_order,
        );
    }
    source_item
}

impl<'request, B: LayoutBufferView> BufferSourceWalk<'request, B> {
    /// Walk with no window context. The redisplay path uses
    /// [`new_for_window`](Self::new_for_window) so overlay `window` properties
    /// are honored, so only focused tests seat a walk this way.
    #[cfg(test)]
    pub(crate) fn new(
        buffer_id: BufferId,
        buffer: &'request B,
        start_charpos: i64,
        text_start_byte: usize,
    ) -> Self {
        Self::new_for_window(buffer_id, buffer, None, start_charpos, text_start_byte)
    }

    pub(crate) fn new_for_window(
        buffer_id: BufferId,
        buffer: &'request B,
        window_id: Option<u64>,
        start_charpos: i64,
        text_start_byte: usize,
    ) -> Self {
        Self {
            producer: BufferElementProducer::new_for_window(
                buffer_id,
                buffer,
                window_id,
                start_charpos,
                text_start_byte,
            ),
            append_state: DisplaySourceRowAppendState::default(),
        }
    }

    pub(crate) fn append_state(&mut self) -> &mut DisplaySourceRowAppendState {
        &mut self.append_state
    }

    pub(crate) fn resolved_source_face(&self, face_id: FaceId) -> Option<&ResolvedFace> {
        self.producer.resolved_source_face(face_id)
    }

    pub(crate) fn remember_resolved_source_face_if_absent(
        &mut self,
        face_id: FaceId,
        face: &ResolvedFace,
    ) {
        self.producer
            .remember_resolved_source_face_if_absent(face_id, face);
    }

    /// Decline run batching until `end_charpos`. See
    /// [`BufferElementProducer::request_char_granularity_until`].
    pub(crate) fn request_char_granularity_until(&mut self, end_charpos: i64) {
        self.producer.request_char_granularity_until(end_charpos);
    }

    /// Consume only a prefix of the element just produced: the producer resumes
    /// at `resume_charpos`. See [`BufferElementProducer::consume_prefix_to`].
    pub(crate) fn consume_prefix_to(&mut self, resume_charpos: i64) {
        self.producer.consume_prefix_to(resume_charpos);
    }

    /// True when the producer's next element at this source position is an
    /// overlay-string insertion rather than buffer text.
    pub(crate) fn has_pending_overlay_strings_at(
        &self,
        source_position: DisplaySourceTextPosition,
    ) -> bool {
        self.producer
            .has_pending_overlay_strings_at(source_position)
    }

    /// Reseat the producer at a row-wrap retry position so the current character
    /// is re-produced on the continuation row.
    pub(crate) fn rewind_source_consumption(&mut self, rewind: BufferSourceRewind) {
        match rewind {
            BufferSourceRewind::WordWrap(position) => self.producer.rewind_word_wrap_to(position),
            BufferSourceRewind::CharacterWrap(position) => {
                self.producer.rewind_character_wrap_to(position)
            }
        }
    }

    pub(crate) fn consume_source_item_for_render(
        &mut self,
        progress: &mut DisplaySourceProgressState<'_>,
        face_resolution_context: BufferSourceFaceResolutionContext<'_, B>,
        face_ids: &mut FrameFaceAttempt,
        source_render: &mut TextRowSourceRenderState<'_>,
        row_geometry: &mut DisplayRowGeometryState,
        append_surface: &DisplayRowAppendSurface,
        active_face_state: &DisplayRowActiveFaceState,
    ) -> Option<BufferSourceConsumedItem> {
        let step = self.producer.produce_step(
            progress.source_position(),
            face_resolution_context,
            face_ids,
        );
        apply_produced_step_to_render_progress(
            step,
            progress,
            face_resolution_context,
            source_render,
            row_geometry,
            append_surface,
            active_face_state,
            face_ids,
        )
    }

    pub(crate) fn consume_hscroll_skip(
        &mut self,
        text: &[u8],
        source_position: DisplaySourceTextPosition,
        hscroll_skip: &mut HorizontalScrollSkipState,
        tab_width: i32,
    ) -> DisplaySourcePositionConsumption<Option<BufferSourceHscrollSkipAction>> {
        let mut source_position = source_position;
        let action =
            consume_hscroll_skip_from_position(text, &mut source_position, hscroll_skip, tab_width);
        DisplaySourcePositionConsumption::new(action, source_position)
    }

    pub(crate) fn consume_invisible_checkpoint(
        &mut self,
        buffer: &B,
        context: BufferSourceInvisibleTextScanContext<'_>,
        checkpoints: &mut InvisibleTextScanCheckpoint,
        source_position: DisplaySourceTextPosition,
    ) -> DisplaySourcePositionConsumption<BufferSourceInvisibleTextScanAction> {
        let mut source_position = source_position;
        let action = context.consume_at_checkpoint(buffer, checkpoints, &mut source_position);
        DisplaySourcePositionConsumption::new(action, source_position)
    }

    pub(crate) fn consume_selective_display_tail(
        &mut self,
        selective_display: BufferSourceSelectiveDisplayContext<'_>,
        source_position: DisplaySourceTextPosition,
    ) -> DisplaySourcePositionConsumption<BufferSourceSelectiveDisplayLineTailAction> {
        let mut source_position = source_position;
        let action =
            selective_display.skip_rest_of_line_after_carriage_return(&mut source_position);
        DisplaySourcePositionConsumption::new(action, source_position)
    }

    pub(crate) fn consume_hidden_indented_lines_after_line_break(
        &mut self,
        selective_display: BufferSourceSelectiveDisplayContext<'_>,
        source_position: DisplaySourceTextPosition,
        line_numbers: &mut LineNumberRenderState,
    ) -> DisplaySourcePositionConsumption<BufferSourceSelectiveDisplayHiddenLines> {
        let mut source_position = source_position;
        let hidden_lines = selective_display
            .apply_hidden_indented_lines_after_line_break(&mut source_position, line_numbers);
        DisplaySourcePositionConsumption::new(hidden_lines, source_position)
    }

    pub(crate) fn consume_truncation_skip(
        &mut self,
        text: &[u8],
        source_position: DisplaySourceTextPosition,
    ) -> DisplaySourcePositionConsumption<BufferSourceTruncationSkipAction> {
        let mut source_position = source_position;
        let action = BufferSourceTruncationSkipAction::consume_source_step_char_and_rest_of_line(
            text,
            &mut source_position,
        );
        DisplaySourcePositionConsumption::new(action, source_position)
    }

    pub(crate) fn source_position_update(
        &mut self,
        source_position: DisplaySourceTextPosition,
    ) -> DisplaySourcePositionConsumption<()> {
        DisplaySourcePositionConsumption::new((), source_position)
    }
}
