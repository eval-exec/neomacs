use crate::display_item::DisplayItem;
use crate::display_source::DisplayItemSource;
use crate::display_source_resolver::{
    DisplaySourceFaceScope, DisplaySourceResolveParams, DisplaySourceResolveState,
    ResolvedDisplaySourceItem, resolve_next_display_source_item,
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

    pub(crate) fn next_resolved_item(
        &mut self,
        source: &mut impl DisplayItemSource,
        params: DisplaySourceResolveParams<'_>,
        face_ids: &mut FrameFaceAttempt,
    ) -> ResolvedDisplaySourceItem {
        if self.is_finished() {
            return ResolvedDisplaySourceItem::empty();
        }
        if let Some(item) = self.take_pending_item() {
            return ResolvedDisplaySourceItem::new(Some(item), Vec::new());
        }
        let mut resolved = resolve_next_display_source_item(
            source,
            self.face_scope,
            params,
            &mut self.resolve_state,
            face_ids,
        );
        self.pending_non_text_area
            .extend(resolved.take_pending_non_text_area());
        if resolved.item().is_none() {
            self.mark_exhausted();
        }
        resolved
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

    pub(crate) fn remember_pending_item(&mut self, item: Option<DisplayItem>) {
        self.pending_item = item;
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
