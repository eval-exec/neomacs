//! Child-frame presentations owned by the compositor.
//!
//! These methods own `FrameCompositor::hidden_child_frames`, which is why they
//! live inside this module rather than in `frame_windows`.
//!
//! Lifecycle animation enters here and nowhere else: a fresh install binds
//! the open slot, a removal binds the close slot and retires the entry into
//! the manager's dying list. Every interaction consumer reads the live map
//! only, so a dying popup cannot be hit-tested, focused, or resolved as a
//! cursor target while it fades.

use std::collections::HashSet;

use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_display_protocol::frame_time::observe_platform_now;
use neomacs_display_protocol::motion_spec::MotionSpec;

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
        // The close slot decides whether the subtree vanishes outright or
        // fades out. Dating to an observed *now* rather than a presentation
        // tick matches where the removal lands: the pixels are already gone
        // from Emacs's model, and the fade is a retainer for what was on
        // screen, not a promise about a future frame.
        let motion = self.compositor.child_frame_motion;
        let (close_spec, close_slide, animates) = match motion.close {
            MotionSpec::Instant => (MotionSpec::Instant, 0.0, false),
            close => (close, motion.close_slide, true),
        };
        tracing::debug!(
            frame_id,
            spec = ?motion.close,
            slide_pixels = motion.close_slide,
            "child_frame_lifecycle: close_animation_resolved"
        );
        let origin = observe_platform_now();
        let removed = if animates {
            self.compositor.child_frames.retire_frame(
                frame_id,
                close_spec,
                origin,
                close_slide,
                motion.close_scale_from,
            )
        } else {
            self.compositor.child_frames.remove_frame(frame_id)
        };
        if removed {
            // The scene changed without an ingest: the retained-static
            // texture still holds the departed frame's pixels at whatever
            // alpha the last build saw, and it must rebuild once the corpse
            // is gone. Bump the generation here so the first eligible frame
            // after the fade rewrites it.
            self.compositor.current_scene_generation =
                crate::render_thread::frame_state::next_scene_generation();
            self.compositor
                .pending_child_frame_removals_to_present
                .push(frame_id);
        }
        tracing::info!(
            frame_id,
            removed,
            animated = animates,
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
        let is_fresh_install = !self.compositor.child_frames.frames.contains_key(&frame_id);
        let previous_presentation = self
            .compositor
            .child_frames
            .frames
            .get(&frame_id)
            .map(|entry| entry.frame.presentation_id);
        let previous_placement = self
            .compositor
            .child_frames
            .frames
            .get(&frame_id)
            .map(|entry| (entry.abs_x, entry.abs_y));
        let next_presentation = frame.presentation_id;
        let changed = self.compositor.child_frames.update_frame(frame);
        if changed {
            #[cfg(feature = "video")]
            self.refresh_visible_videos();
            self.compositor.dirty = true;
            // A fresh install is the appearance trigger. It fires on the
            // install event, never on the content refreshes a completion
            // popup produces while typing, so the fade cannot restart
            // mid-flight: a payload update carries the previous animation
            // forward instead.
            if is_fresh_install {
                let motion = self.compositor.child_frame_motion;
                if !motion.open.is_instant() {
                    tracing::debug!(
                        frame_id,
                        spec = ?motion.open,
                        slide_pixels = motion.open_slide,
                        "child_frame_lifecycle: open_animation_started"
                    );
                    self.compositor.child_frames.begin_open_animation(
                        frame_id,
                        motion.open,
                        observe_platform_now(),
                        motion.open_slide,
                        motion.open_scale_from,
                    );
                    self.compositor.dirty = true;
                }
            }
            // A re-anchor is the movement trigger: the payload's placed
            // position moved on an entry that already exists. It fires on
            // the placement delta, not on content refreshes at the same
            // anchor. A drift already in flight retargets from the position
            // the last pass painted, at the speed it had; a fresh one
            // departs from the placement the popup held until now.
            if !is_fresh_install
                && let Some((previous_x, previous_y)) = previous_placement
                && let Some(entry) = self.compositor.child_frames.frames.get(&frame_id)
                && ((entry.abs_x - previous_x).abs() > f32::EPSILON
                    || (entry.abs_y - previous_y).abs() > f32::EPSILON)
            {
                let motion = self.compositor.child_frame_motion;
                if !motion.movement.is_instant() {
                    let retargeted = self.compositor.child_frames.retarget_drift(
                        frame_id,
                        motion.movement,
                        observe_platform_now(),
                    );
                    if !retargeted {
                        self.compositor.child_frames.begin_drift(
                            frame_id,
                            previous_x,
                            previous_y,
                            motion.movement,
                            observe_platform_now(),
                        );
                    }
                    self.compositor.dirty = true;
                }
            }
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
