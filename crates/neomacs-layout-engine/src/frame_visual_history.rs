//! Accepted visual history used to derive transitions and presentation effects.
//!
//! GNU redisplay owns current/desired matrices on each `struct frame`; a child
//! frame is redisplayed independently and never replaces its parent's history.
//! Keep the same invariant here by making `FrameId` mandatory at the history
//! store interface.  A speculative layout reads an immutable snapshot and only
//! an accepted presentation is committed.

use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::types::DisplayWindowId;
use neovm_core::window::FrameId;
use rustc_hash::FxHashMap;

#[derive(Clone, Debug, Default)]
pub(crate) struct FrameVisualHistory {
    window_infos: FxHashMap<DisplayWindowId, WindowInfo>,
}

impl FrameVisualHistory {
    pub(crate) fn from_accepted_presentation(
        window_infos: FxHashMap<DisplayWindowId, WindowInfo>,
    ) -> Self {
        Self { window_infos }
    }

    pub(crate) fn window_infos(&self) -> &FxHashMap<DisplayWindowId, WindowInfo> {
        &self.window_infos
    }
}

#[derive(Debug, Default)]
pub(crate) struct FrameVisualHistories {
    by_frame: FxHashMap<FrameId, FrameVisualHistory>,
}

impl FrameVisualHistories {
    /// Snapshot the last accepted presentation for exactly one logical frame.
    pub(crate) fn snapshot(&self, frame_id: FrameId) -> FrameVisualHistory {
        self.by_frame.get(&frame_id).cloned().unwrap_or_default()
    }

    /// Commit history only after the matching frame presentation has sealed.
    pub(crate) fn commit(&mut self, frame_id: FrameId, history: FrameVisualHistory) {
        self.by_frame.insert(frame_id, history);
    }
}
