//! Child-frame presentations owned by the compositor.
//!
//! These methods own `FrameCompositor::hidden_child_frames`, which is why they
//! live inside this module rather than in `frame_windows`.

use std::collections::HashSet;

use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::render_thread::frame_windows::GuiFrameRenderState;

impl GuiFrameRenderState {
    pub(in crate::render_thread) fn remove_child_frame(&mut self, frame_id: u64) -> bool {
        let before = self.active_pointer_damage();
        let removed_ids = self.compositor.child_frames.subtree_frame_ids(frame_id);
        let removed_presentations = removed_ids
            .iter()
            .filter_map(|id| self.compositor.child_frames.frames.get(id))
            .map(|entry| entry.frame.presentation_id)
            .collect::<Vec<_>>();
        self.compositor.hidden_child_frames.insert(frame_id);
        let removed = self.compositor.child_frames.remove_frame(frame_id);
        if removed {
            self.compositor
                .pending_child_frame_removals_to_present
                .push(frame_id);
        }
        tracing::info!(
            frame_id,
            removed,
            "child_frame_lifecycle: compositor_remove"
        );
        if removed {
            #[cfg(feature = "video")]
            self.refresh_visible_videos();
            self.compositor.dirty = true;
            for presentation in removed_presentations {
                if self.pointer_appearance.retire(presentation) {
                    self.record_pointer_paint_transition(before);
                }
            }
        }
        if self
            .cursor
            .target_cloned()
            .is_some_and(|target| removed_ids.contains(&target.frame_id))
        {
            self.cursor.clear_target();
            self.input_method.clear();
            self.compositor.dirty = true;
            return true;
        }
        removed
    }

    pub(in crate::render_thread) fn displayed_presentations(&self) -> HashSet<u64> {
        let mut presentations = HashSet::new();
        if let Some(presentation) = self
            .compositor
            .current_frame
            .as_ref()
            .map(|frame| frame.presentation_id.get())
            .filter(|presentation| *presentation != 0)
        {
            presentations.insert(presentation);
        }
        presentations.extend(
            self.compositor
                .child_frames
                .frames
                .values()
                .map(|entry| entry.frame.presentation_id.get())
                .filter(|presentation| *presentation != 0),
        );
        presentations
    }

    #[allow(dead_code)] // direct single-child lookup remains covered by frame_windows tests
    pub(in crate::render_thread) fn child_presentation(&self, frame_id: u64) -> Option<u64> {
        self.compositor
            .child_frames
            .frames
            .get(&frame_id)
            .map(|entry| entry.frame.presentation_id.get())
            .filter(|presentation| *presentation != 0)
    }

    pub(in crate::render_thread) fn child_subtree_presentations(&self, frame_id: u64) -> Vec<u64> {
        self.compositor.child_frames.subtree_presentations(frame_id)
    }

    pub(in crate::render_thread) fn show_child_frame(&mut self, frame_id: u64) -> bool {
        let changed = self.compositor.hidden_child_frames.remove(&frame_id);
        tracing::info!(frame_id, changed, "child_frame_lifecycle: compositor_show");
        changed
    }

    pub(in crate::render_thread) fn update_child_frame(&mut self, frame: FrameGlyphBuffer) -> bool {
        let before = self.active_pointer_damage();
        let frame_id = frame.frame_placement.frame().get();
        if self.compositor.hidden_child_frames.contains(&frame_id) {
            tracing::debug!(
                frame_id,
                "ignoring child frame update while frame is explicitly hidden"
            );
            return false;
        }
        let previous_presentation = self
            .compositor
            .child_frames
            .frames
            .get(&frame_id)
            .map(|entry| entry.frame.presentation_id);
        let next_presentation = frame.presentation_id;
        let changed = self.compositor.child_frames.update_frame(frame);
        if changed {
            #[cfg(feature = "video")]
            self.refresh_visible_videos();
            self.compositor.dirty = true;
            if let Some(previous) = previous_presentation
                && previous != next_presentation
                && self.pointer_appearance.retire(previous)
            {
                self.record_pointer_paint_transition(before);
            }
        }
        changed
    }
}
