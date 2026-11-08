//! Accepted visual history used to derive transitions and presentation effects.
//!
//! GNU redisplay owns current/desired matrices on each `struct frame`; a child
//! frame is redisplayed independently and never replaces its parent's history.
//! Keep the same invariant here by making `FrameId` mandatory at the history
//! store interface.  A speculative layout reads an immutable snapshot and only
//! an accepted presentation is committed.

use crate::display_when::{DisplayWhenRecord, DisplayWhenRetention};
use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::types::DisplayWindowId;
use neovm_core::window::FrameId;
use rustc_hash::FxHashMap;

#[derive(Clone, Debug, Default)]
pub(crate) struct FrameVisualHistory {
    window_infos: FxHashMap<DisplayWindowId, WindowInfo>,
    /// What each accepted window's rows were built from for its `when`
    /// display clauses.  This is the dependency record the retained-row
    /// machinery consults; it is committed with the presentation that sealed
    /// it, so speculative attempts never publish it.  It lives here (inside a
    /// heap-backed per-frame history) and not in `RetainedWindowMatrix`: that
    /// type is embedded inline in the thread-local `ScrollCoverage`, so
    /// growing it would move every `%fs` offset in the binary.
    display_when: FxHashMap<DisplayWindowId, DisplayWhenRetention>,
    cursor_sources:
        FxHashMap<DisplayWindowId, crate::cursor_source_dependencies::CursorSourceDependencies>,
}

impl FrameVisualHistory {
    pub(crate) fn from_accepted_presentation(
        window_infos: FxHashMap<DisplayWindowId, WindowInfo>,
        display_when: FxHashMap<DisplayWindowId, DisplayWhenRetention>,
    ) -> Self {
        Self {
            window_infos,
            display_when,
            cursor_sources: FxHashMap::default(),
        }
    }

    /// Attach owner-only source certificates to this accepted presentation,
    /// without growing TLS-embedded matrices or public numeric snapshots.
    pub(crate) fn with_cursor_sources(
        mut self,
        sources: FxHashMap<
            DisplayWindowId,
            crate::cursor_source_dependencies::CursorSourceDependencies,
        >,
    ) -> Self {
        self.cursor_sources = sources;
        self
    }

    /// Clone the original certificate for exactly this accepted window.
    pub(crate) fn cursor_sources_for(
        &self,
        window: DisplayWindowId,
    ) -> Option<crate::cursor_source_dependencies::CursorSourceDependencies> {
        self.cursor_sources.get(&window).cloned()
    }

    pub(crate) fn window_infos(&self) -> &FxHashMap<DisplayWindowId, WindowInfo> {
        &self.window_infos
    }

    /// The `when` record accepted for WINDOW, if its walk settled one.
    pub(crate) fn display_when_for(&self, window: DisplayWindowId) -> Option<DisplayWhenRetention> {
        if self.display_when.is_empty() {
            return None;
        }
        self.display_when.get(&window).copied()
    }

    /// Whether any accepted window carries rows whose `when` answers stayed
    /// unverified; such rows are never reused.
    pub(crate) fn any_display_when_unverified(&self) -> bool {
        self.display_when
            .values()
            .any(|retention| retention.record == DisplayWhenRecord::Unknown)
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

    /// The last accepted history for exactly one logical frame, by reference.
    pub(crate) fn get(&self, frame_id: FrameId) -> Option<&FrameVisualHistory> {
        self.by_frame.get(&frame_id)
    }

    /// Closing a frame retires its original string roots with its history.
    pub(crate) fn retain_live(&mut self, mut live: impl FnMut(FrameId) -> bool) {
        self.by_frame.retain(|frame, _| live(*frame));
    }

    /// Commit history only after the matching frame presentation has sealed.
    pub(crate) fn commit(&mut self, frame_id: FrameId, history: FrameVisualHistory) {
        self.by_frame.insert(frame_id, history);
    }
}
