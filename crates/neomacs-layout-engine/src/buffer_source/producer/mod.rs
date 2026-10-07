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

impl ProducedStep {
    pub(crate) fn empty(source_position: DisplaySourceTextPosition) -> Self {
        Self {
            source_item: None,
            source_position,
            pending_faces: Vec::new(),
            pending_non_text_area: Vec::new(),
        }
    }
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
        Self::new_for_window_range(
            buffer_id,
            buffer,
            window_id,
            start_charpos,
            CharPos0::new(usize::MAX),
            text_start_byte,
        )
    }

    /// Bound acquisition before a text run can allocate its source payload.
    /// Reaching this boundary is exhaustion, not an artificial row break.
    pub(crate) fn new_for_window_range(
        buffer_id: BufferId,
        buffer: &'request B,
        window_id: Option<u64>,
        start_charpos: i64,
        end: CharPos0,
        text_start_byte: usize,
    ) -> Self {
        Self {
            source_cursor: BufferTextSourceCursor::new_for_window(
                buffer_id,
                buffer,
                window_id,
                CharPos0::new(start_charpos.max(0) as usize),
                end,
                RenderFaceRef::Inherit,
            ),
            source_resolve_state: DisplaySourceResolveState::default(),
            source_consumption: BufferSourceConsumptionState::new(text_start_byte),
        }
    }

    /// The buffer view the producer walks, for callers that must consult
    /// buffer state directly (the hscroll skip resolving `display` specs).
    pub(crate) fn layout_buffer(&self) -> &B {
        self.source_cursor.layout_buffer()
    }

    /// The Emacs byte position of `text[0]` in the buffer.
    pub(crate) fn text_start_byte(&self) -> usize {
        self.source_consumption.text_start_byte()
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

    pub(crate) fn produces_single_chars_at(&self, charpos: i64) -> bool {
        self.source_cursor
            .produces_single_chars_at(CharPos0::new(charpos.max(0) as usize))
    }

    pub(crate) fn can_restart_after_buffer_newline(&self, charpos: i64) -> bool {
        usize::try_from(charpos).is_ok_and(|position| {
            self.source_cursor
                .can_restart_after_buffer_newline(CharPos0::new(position))
        })
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

    /// Produce directly into storage owned by the dispatcher, keeping the
    /// large consumed-item enum in that storage while side effects are applied.
    pub(crate) fn produce_step_into(
        &mut self,
        source_position: DisplaySourceTextPosition,
        face_resolution_context: BufferSourceFaceResolutionContext<'_, B>,
        face_ids: &mut FrameFaceAttempt,
        output: &mut ProducedStep,
    ) {
        output.source_item = None;
        output.source_position = source_position;
        output.pending_faces.clear();
        output.pending_non_text_area.clear();
        let params = face_resolution_context.source_resolve_params(None);
        let mut resolver = DisplaySourcePropertyResolver::buffer_local(
            face_resolution_context.buffer(),
            params,
            &mut self.source_resolve_state,
            face_ids,
            &mut output.pending_faces,
        );
        let mut source_context = DisplaySourceContext::with_face_resolver_and_non_text_area_sink(
            &mut resolver,
            &mut output.pending_non_text_area,
            crate::display_property::DisplayPropertyTarget::for_window_system(
                params.face_basis().face_resolver().is_window_system(),
            ),
        )
        .with_automatic_composition(params.automatic_composition);
        output.source_item = self.source_consumption.next_source_consumption_item(
            &mut self.source_cursor,
            &mut source_context,
            &mut output.source_position,
        );
    }

    /// Owned acquisition still consumes the complete produced step. The
    /// interactive dispatcher uses [`Self::produce_step_into`] instead.
    pub(crate) fn produce_step(
        &mut self,
        source_position: DisplaySourceTextPosition,
        face_resolution_context: BufferSourceFaceResolutionContext<'_, B>,
        face_ids: &mut FrameFaceAttempt,
    ) -> ProducedStep {
        let mut output = ProducedStep::empty(source_position);
        self.produce_step_into(
            source_position,
            face_resolution_context,
            face_ids,
            &mut output,
        );
        output
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
            crate::display_property::DisplayPropertyTarget::for_window_system(
                params.face_basis().face_resolver().is_window_system(),
            ),
        )
        .with_automatic_composition(buffer.layout_string_composition_rules());
        self.source_consumption.next_source_consumption_item(
            &mut self.source_cursor,
            &mut context,
            position,
        )
    }
}

#[cfg(test)]
#[path = "tests/producer_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/stream_harness_test.rs"]
mod stream_harness_tests;
