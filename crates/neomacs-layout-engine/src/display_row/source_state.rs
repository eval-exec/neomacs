use crate::display_item::DisplayItem;
use crate::display_row::render_item::DisplayRowClippedRemainder;
use crate::display_source::DisplayItemSource;
use crate::display_source_resolver::{
    DisplaySourceFaceScope, DisplaySourceResolveParams, DisplaySourceResolveState,
    ResolvedDisplaySourceItem, resolve_next_display_source_item_into,
};
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::types::FaceId;

pub(crate) struct DisplayRowSourceState {
    face_scope: DisplaySourceFaceScope,
    resolve_state: DisplaySourceResolveState,
    pending_item: Option<DisplayItem>,
    exhausted: bool,
    /// Typed output collected outside the text flow while resolving this
    /// source's items, drained by the render path onto the output row.
    pending_non_text_area: Vec<crate::display_source::DisplayNonTextAreaEmission>,
}

impl DisplayRowSourceState {
    pub(crate) fn frame_local() -> Self {
        Self::with_face_scope(DisplaySourceFaceScope::FrameLocal)
    }

    pub(crate) fn with_face_scope(face_scope: DisplaySourceFaceScope) -> Self {
        Self {
            face_scope,
            resolve_state: DisplaySourceResolveState::default(),
            pending_item: None,
            exhausted: false,
            pending_non_text_area: Vec::new(),
        }
    }

    pub(crate) fn face_scope(&self) -> DisplaySourceFaceScope {
        self.face_scope
    }

    pub(crate) fn next_resolved_item_into(
        &mut self,
        source: &mut impl DisplayItemSource,
        params: DisplaySourceResolveParams<'_>,
        face_ids: &mut FrameFaceAttempt,
        output: &mut ResolvedDisplaySourceItem,
    ) {
        if self.is_finished() {
            output.clear();
            return;
        }
        if let Some(item) = self.take_pending_item() {
            output.set_pending_item(item);
            return;
        }
        resolve_next_display_source_item_into(
            source,
            self.face_scope,
            params,
            &mut self.resolve_state,
            face_ids,
            output,
        );
        self.pending_non_text_area
            .extend(output.take_pending_non_text_area());
        if output.item().is_none() {
            self.mark_exhausted();
        }
    }

    #[cfg(test)]
    pub(crate) fn next_resolved_item(
        &mut self,
        source: &mut impl DisplayItemSource,
        params: DisplaySourceResolveParams<'_>,
        face_ids: &mut FrameFaceAttempt,
    ) -> ResolvedDisplaySourceItem {
        let mut output = ResolvedDisplaySourceItem::empty();
        self.next_resolved_item_into(source, params, face_ids, &mut output);
        output
    }

    /// Drain non-text-area output collected while resolving this source.
    pub(crate) fn take_pending_non_text_area(
        &mut self,
    ) -> Vec<crate::display_source::DisplayNonTextAreaEmission> {
        std::mem::take(&mut self.pending_non_text_area)
    }

    pub(crate) fn resolved_face(&self, face_id: FaceId) -> Option<&ResolvedFace> {
        self.resolve_state.resolved_face(face_id)
    }

    fn take_pending_item(&mut self) -> Option<DisplayItem> {
        self.pending_item.take()
    }

    /// Carry the part of the clipped item the next row still owes.
    ///
    /// Both resumable cases become the head of the next row: GNU resumes the
    /// element's unrendered tail there, and re-produces an element it removed
    /// whole (`display_line`, src/xdisp.c:26448-26475).  `Nothing` is the only
    /// way to remember no item, so a clipped item cannot disappear for want of
    /// a representation.
    pub(crate) fn remember_pending_item(&mut self, remainder: DisplayRowClippedRemainder) {
        self.pending_item = match remainder {
            DisplayRowClippedRemainder::Resume(item)
            | DisplayRowClippedRemainder::DeferWhole(item) => Some(item),
            DisplayRowClippedRemainder::Nothing => None,
        };
    }

    pub(crate) fn discard_pending_item(&mut self) {
        self.pending_item = None;
    }

    fn mark_exhausted(&mut self) {
        self.exhausted = true;
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.exhausted && self.pending_item.is_none()
    }
}

#[cfg(test)]
#[path = "source_state/tests/source_state_test.rs"]
mod tests;
