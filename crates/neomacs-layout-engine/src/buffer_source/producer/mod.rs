//! The buffer element production nucleus.
//!
//! [`BufferElementProducer`] is the sole owner of the buffer-text cursor, the
//! display-source resolve state, and the consumption state. Everything outside
//! it — row assembly, the
//! append surface, overflow decisions — is renderer state and lives on
//! [`BufferSourceWalk`](crate::buffer_source::walk::BufferSourceWalk).
//!
//! Two rewind mechanisms exist, and they are not interchangeable:
//!
//! * [`BufferElementProducer::rewind_word_wrap_to`] and
//!   [`BufferElementProducer::rewind_character_wrap_to`] reseat the producer at
//!   a published source position. The row-wrap retry selects between them
//!   ([`BufferSourceWalk::rewind_source_consumption`](crate::buffer_source::walk::BufferSourceWalk::rewind_source_consumption)).
//! * [`ProducerSnapshot`] saves the producer's whole seating opaquely,
//!   mirroring GNU's `SAVE_IT` / `RESTORE_IT`. No production path calls it:
//!   its consumers are the stream-equivalence harness and the producer unit
//!   tests, which need to replay a producer from an exact seating to compare
//!   two walks. It is `#[cfg(test)]` for exactly that reason — harness support,
//!   not a production mechanism.

pub(crate) mod frame;
pub(crate) mod vocabulary;

use crate::buffer_source::consumption::{BufferSourceConsumedItem, BufferSourceConsumptionState};
use crate::buffer_source::face_resolution::BufferSourceFaceResolutionContext;
use crate::buffer_source::text_source::BufferTextSourceCursor;
use crate::display_item::RenderFaceRef;
use crate::display_source::{
    DisplayNonTextAreaEmission, DisplaySourceContext, DisplaySourceTextPosition,
};
use crate::display_source_resolver::{
    DisplaySourcePropertyResolver, DisplaySourceResolveState, PendingDisplaySourceFace,
};
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{LayoutBufferView, ResolvedFace};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{BufferId, CharPos0};

/// An opaque save of the producer's whole seating. Restoring one reinstates the
/// producer exactly, which a bare position rewind cannot express. Harness and
/// unit-test support only — see the module docs for why the wrap retry uses
/// the typed position-rewind methods instead.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProducerSnapshot {
    cursor_char_pos: CharPos0,
    char_granularity_end: Option<CharPos0>,
    consumption: BufferSourceConsumptionState,
}

/// What one production step yielded: the element (if any), the walk position it
/// left behind, and the side effects the resolver collected while producing it.
pub(crate) struct ProducedStep {
    pub(crate) source_item: Option<BufferSourceConsumedItem>,
    pub(crate) source_position: DisplaySourceTextPosition,
    pub(crate) pending_faces: Vec<PendingDisplaySourceFace>,
    pub(crate) pending_non_text_area: Vec<DisplayNonTextAreaEmission>,
}

pub(crate) struct BufferElementProducer<'request, B: LayoutBufferView> {
    source_cursor: BufferTextSourceCursor<'request, B>,
    source_resolve_state: DisplaySourceResolveState,
    source_consumption: BufferSourceConsumptionState,
}

impl<'request, B: LayoutBufferView> BufferElementProducer<'request, B> {
    /// Producer with no window context. The redisplay path uses
    /// [`new_for_window`](Self::new_for_window) so overlay `window` properties
    /// are honored, so only focused tests seat a producer this way.
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
            source_cursor: BufferTextSourceCursor::new_for_window(
                buffer_id,
                buffer,
                window_id,
                CharPos0::new(start_charpos.max(0) as usize),
                CharPos0::new(usize::MAX),
                RenderFaceRef::Inherit,
            ),
            source_resolve_state: DisplaySourceResolveState::default(),
            source_consumption: BufferSourceConsumptionState::new(text_start_byte),
        }
    }

    /// Save the producer's seating for a later [`restore`](Self::restore).
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> ProducerSnapshot {
        ProducerSnapshot {
            cursor_char_pos: self.source_cursor.current_char_pos(),
            char_granularity_end: self.source_cursor.char_granularity_end(),
            consumption: self.source_consumption.clone(),
        }
    }

    /// Reinstate a saved seating (GNU `RESTORE_IT`).
    #[cfg(test)]
    pub(crate) fn restore(&mut self, snapshot: ProducerSnapshot) {
        let ProducerSnapshot {
            cursor_char_pos,
            char_granularity_end,
            consumption,
        } = snapshot;
        self.source_consumption = consumption;
        self.source_cursor
            .set_char_granularity_end(char_granularity_end);
        self.source_cursor.reset_to(cursor_char_pos);
    }

    /// Reseat the producer at a word-wrap checkpoint and resume with runs
    /// BATCHED again. GNU RESTORE_IT restores the complete iterator, so
    /// insertion elements produced after the checkpoint must replay too.
    ///
    /// Dropping the batching decline here is the faithful conversion of what the
    /// pending queue's clear did at these same two call sites: the continuation
    /// row measures from a fresh pen, so the remainder of a run refused on the
    /// previous row may well fit whole and must get the chance to.
    pub(crate) fn rewind_word_wrap_to(&mut self, source_position: DisplaySourceTextPosition) {
        self.source_cursor.set_char_granularity_end(None);
        self.source_cursor
            .rewind_for_word_wrap_to(CharPos0::new(source_position.charpos().max(0) as usize));
    }

    /// Retry one overflowing buffer character without replaying insertion
    /// elements that were already drawn before it.
    pub(crate) fn rewind_character_wrap_to(&mut self, source_position: DisplaySourceTextPosition) {
        self.source_cursor.set_char_granularity_end(None);
        self.source_cursor
            .reset_to(CharPos0::new(source_position.charpos().max(0) as usize));
    }

    /// Consume only a PREFIX of the element just produced: reseat the cursor at
    /// `resume_charpos` so the next element begins there.
    ///
    /// The producer-side of GNU's `set_iterator_to_next` — the cursor position
    /// IS the resume state, so nothing is queued. It replaces the fit split,
    /// which rendered a fitting prefix and pushed the unrendered tail back into
    /// `pending_render_items` for the next loop iteration to pop.
    pub(crate) fn consume_prefix_to(&mut self, resume_charpos: i64) {
        self.source_cursor
            .reset_to(CharPos0::new(resume_charpos.max(0) as usize));
    }

    /// Decline run batching until `end_charpos`: the renderer is about to render
    /// this run character by character, so producing the remainder as one run
    /// only to re-measure and re-split it per character is wasted work. See
    /// `BufferTextSourceCursor::char_granularity_end` for why this is a hint
    /// rather than state the output depends on.
    pub(crate) fn request_char_granularity_until(&mut self, end_charpos: i64) {
        self.source_cursor
            .request_char_granularity_until(CharPos0::new(end_charpos.max(0) as usize));
    }

    /// Whether production at `source_position` must first yield an anchored
    /// overlay-string insertion.  This does not move or mark the cursor; the
    /// regular production step remains the sole consumer of the element.
    pub(crate) fn has_pending_overlay_strings_at(
        &self,
        source_position: DisplaySourceTextPosition,
    ) -> bool {
        self.source_cursor
            .has_pending_overlay_strings_at(
                CharPos0::new(source_position.charpos().max(0) as usize),
            )
    }

    pub(crate) fn resolved_source_face(&self, face_id: FaceId) -> Option<&ResolvedFace> {
        self.source_resolve_state.resolved_face(face_id)
    }

    pub(crate) fn remember_resolved_source_face_if_absent(
        &mut self,
        face_id: FaceId,
        face: &ResolvedFace,
    ) {
        if self.source_resolve_state.resolved_face(face_id).is_none() {
            self.source_resolve_state.remember_face(face_id, face);
        }
    }

    /// Produce the next element at `source_position`, resolving faces and
    /// fringe specs into the returned step.
    pub(crate) fn produce_step(
        &mut self,
        mut source_position: DisplaySourceTextPosition,
        face_resolution_context: BufferSourceFaceResolutionContext<'_, B>,
        face_ids: &mut FrameFaceAttempt,
    ) -> ProducedStep {
        let mut pending_faces = Vec::new();
        let mut pending_non_text_area = Vec::new();
        let source_item = {
            let params = face_resolution_context.source_resolve_params(None);
            let mut resolver = DisplaySourcePropertyResolver::buffer_local(
                face_resolution_context.buffer(),
                params,
                &mut self.source_resolve_state,
                face_ids,
                &mut pending_faces,
            );
            let mut source_context =
                DisplaySourceContext::with_face_resolver_and_non_text_area_sink(
                    &mut resolver,
                    &mut pending_non_text_area,
                );
            self.source_consumption.next_source_consumption_item(
                &mut self.source_cursor,
                &mut source_context,
                &mut source_position,
            )
        };
        ProducedStep {
            source_item,
            source_position,
            pending_faces,
            pending_non_text_area,
        }
    }

    /// Produce the next element against a caller-supplied source context, with
    /// no face resolver attached. Focused tests use this to observe the raw
    /// element stream.
    #[cfg(test)]
    pub(crate) fn next_consumed_item(
        &mut self,
        context: &mut DisplaySourceContext<'_>,
        position: &mut DisplaySourceTextPosition,
    ) -> Option<BufferSourceConsumedItem> {
        self.source_consumption.next_source_consumption_item(
            &mut self.source_cursor,
            context,
            position,
        )
    }

    /// Produce the next element with face resolution wired to `face_basis`,
    /// without the renderer-side context [`produce_step`](Self::produce_step)
    /// needs. The stream-equivalence harness drives the producer this way.
    #[cfg(test)]
    pub(crate) fn next_consumed_item_with_face_basis(
        &mut self,
        buffer: &B,
        face_basis: crate::display_source_resolver::DisplaySourceFaceBasis<'_>,
        face_ids: &mut FrameFaceAttempt,
        position: &mut DisplaySourceTextPosition,
    ) -> Option<BufferSourceConsumedItem> {
        let mut pending_faces = Vec::new();
        let mut pending_non_text_area = Vec::new();
        let params = crate::display_source_resolver::DisplaySourceResolveParams::new(
            face_basis,
            None,
            Default::default(),
        );
        let mut resolver = DisplaySourcePropertyResolver::buffer_local(
            buffer,
            params,
            &mut self.source_resolve_state,
            face_ids,
            &mut pending_faces,
        );
        let mut context = DisplaySourceContext::with_face_resolver_and_non_text_area_sink(
            &mut resolver,
            &mut pending_non_text_area,
        );
        self.source_consumption.next_source_consumption_item(
            &mut self.source_cursor,
            &mut context,
            position,
        )
    }
}

#[cfg(test)]
#[path = "producer_test.rs"]
mod tests;

#[cfg(test)]
#[path = "stream_harness_test.rs"]
mod stream_harness_tests;
