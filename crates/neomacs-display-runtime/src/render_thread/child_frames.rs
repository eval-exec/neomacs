//! Child frame management for the render thread.
//!
//! Manages child frames (posframe, which-key-posframe, etc.) as floating
//! overlays composited on top of the parent frame within a single winit window.
//!
//! Lifecycle animations live beside the presentation, not inside it: an
//! entry carries an optional [`EntryAnimation`] (a sampler bound to the
//! moment the frame appeared), and a frame deleted while a close slot is
//! enabled moves into [`ChildFrameManager::dying`] instead of vanishing --
//! pixels retained for the fade, but absent from every interaction path,
//! which consult only [`ChildFrameManager::frames`].

use std::collections::HashMap;

use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
use neomacs_display_protocol::motion_spec::MotionSpec;
use neomacs_renderer_wgpu::SnapshotLease;

use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::render_thread::frame_compositor::motion::{Motion, ProgressRate};
use neomacs_display_protocol::{
    PlaceChildQuery, PresentedClip, PresentedFramePlacement, PresentedFrameScene,
};

/// An animation bound to one child frame's lifecycle.
///
/// The sampler is a [`Motion`]: same spec, same instant, same answer, so a
/// frame redrawn at the same timestamp draws the same picture regardless of
/// how many frames came between.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntryAnimation {
    pub(in crate::render_thread) motion: Motion,
    /// How far the frame displaces vertically while it animates, in logical
    /// pixels. Zero is a pure fade.
    pub slide_pixels: f32,
    /// The scale the frame starts from; 1.0 scales nothing. The draw pass
    /// turns this into the frame's interpolated scale around its anchor.
    pub scale_from: f32,
    /// Whether the frame is leaving (`true`) or arriving (`false`).
    ///
    /// One direction flag instead of two animation fields: the close path
    /// shows `1 - progress`, the open path shows `progress`, and everything
    /// else about the curves is identical.
    pub closing: bool,
}

impl EntryAnimation {
    /// Bind a spec to the instant it started, or keep nothing for an
    /// `Instant` spec -- the type-level statement that disabled animation
    /// builds no state at all.
    pub(crate) fn start(
        spec: MotionSpec,
        origin: EventTime,
        slide_pixels: f32,
        scale_from: f32,
        closing: bool,
    ) -> Option<Self> {
        Motion::start(spec, origin).map(|motion| Self {
            motion,
            slide_pixels,
            scale_from,
            closing,
        })
    }
}

/// State for one child frame.
pub(crate) struct ChildFrameEntry {
    pub frame_id: u64,
    pub frame: FrameGlyphBuffer,
    /// Last applied native opacity; GNU nil retains this across payloads.
    pub applied_frame_alpha: f32,
    /// Computed absolute position on screen (from parent_x/parent_y)
    pub abs_x: f32,
    pub abs_y: f32,
    pub clip_in_root: PresentedClip,
    pub z_path: Vec<i32>,
    /// Frame counter when this entry was last updated
    #[allow(dead_code)] // read by the test-exercised prune_stale
    pub last_updated: u64,
    /// Unique per-install stamp; the face aggregation signature uses it to
    /// detect that this entry's frame payload was replaced.
    pub ingest_seq: u64,
    /// The lifecycle animation in progress, if one was started.
    pub animation: Option<EntryAnimation>,
    /// A content crossfade from the previous presentation's picture, when a
    /// size-changing update arrived with the resize slot enabled.
    pub crossfade: Option<EntryCrossfade>,
    /// A placement drift toward the entry's settled position, when a
    /// re-anchoring update moved the popup with the movement slot enabled.
    pub drift: Option<EntryDrift>,
}

/// A content crossfade between the previous presentation's picture and the
/// freshly installed one.
pub(crate) struct EntryCrossfade {
    pub(in crate::render_thread) motion: Motion,
    /// The old content's picture, leased from the snapshot pool. Dropping
    /// the entry returns the lease.
    pub(in crate::render_thread) old: SnapshotLease,
    /// The old content's logical size, which may differ from the new
    /// frame's.
    pub(in crate::render_thread) old_width: f32,
    pub(in crate::render_thread) old_height: f32,
}

/// A placement drift: the popup gliding from where it was drawn to where
/// Emacs just anchored it.
///
/// `from_x`/`from_y` is the departure, in logical surface coordinates; the
/// destination is always the entry's own placed position, so a payload that
/// changes both placement and content needs no separate target. When a new
/// re-anchor arrives mid-drift, the drift retargets from `last_drawn` --
/// the position the previous pass actually painted -- so the popup carries
/// on from where the user saw it instead of snapping back to a placement it
/// never reached.
pub(crate) struct EntryDrift {
    pub(in crate::render_thread) motion: Motion,
    pub(in crate::render_thread) from_x: f32,
    pub(in crate::render_thread) from_y: f32,
    /// The position and rate the last scene pass painted, written back for
    /// the next retarget. `None` until the first pass after the drift began.
    pub(in crate::render_thread) last_drawn: Option<(f32, f32, f32)>,
}

/// A child frame whose deletion is animating out.
pub(crate) struct DyingChildFrame {
    pub entry: ChildFrameEntry,
    pub animation: EntryAnimation,
}

/// Manages all child frames for the render thread.
pub(crate) struct ChildFrameManager {
    pub frames: HashMap<u64, ChildFrameEntry>,
    /// Frame IDs sorted by z_order for rendering (lowest first = back-most)
    render_order: Vec<u64>,
    /// Monotonic counter incremented each poll_frame cycle
    frame_counter: u64,
    root: Option<PresentedFramePlacement>,
    /// Frames removed while a close slot is enabled, kept for their fade.
    ///
    /// A `Vec` because the set is tiny (at most one per popup dismissal) and
    /// order is z-order at the moment of death; interaction paths never read
    /// it, so insertion cost is irrelevant.
    dying: Vec<DyingChildFrame>,
}

impl ChildFrameManager {
    pub fn new() -> Self {
        Self {
            frames: HashMap::new(),
            render_order: Vec::new(),
            frame_counter: 0,
            root: None,
            dying: Vec::new(),
        }
    }

    pub fn set_root_frame(&mut self, root: Option<&FrameGlyphBuffer>) {
        self.root = root.map(|root| root.frame_placement);
        self.rebuild_presented_scene();
    }

    /// Increment the frame counter. Call once per poll_frame cycle.
    pub fn tick(&mut self) {
        self.frame_counter += 1;
    }

    /// Record that `frame_id` was installed fresh, binding its open slot.
    ///
    /// The caller decides "fresh" (it sees the map before the install);
    /// re-delivering an existing frame must not retrigger an appearance.
    pub fn begin_open_animation(
        &mut self,
        frame_id: u64,
        spec: MotionSpec,
        origin: EventTime,
        slide_pixels: f32,
        scale_from: f32,
    ) {
        if let Some(entry) = self.frames.get_mut(&frame_id)
            && let Some(animation) =
                EntryAnimation::start(spec, origin, slide_pixels, scale_from, false)
        {
            entry.animation = Some(animation);
        }
    }

    /// Remove `frame_id`'s subtree, retaining each removed entry in `dying`
    /// with a bound close animation.
    ///
    /// Frames whose close slot resolves to `Instant` are dropped outright --
    /// that is the "no animation" case behaving exactly as removal always
    /// did. A dying entry keeps its placement, clip and z-path snapshots; it
    /// is unconsultable for interaction, which is the whole point of keeping
    /// it out of `frames`.
    pub fn retire_frame(
        &mut self,
        frame_id: u64,
        spec: MotionSpec,
        origin: EventTime,
        slide_pixels: f32,
        scale_from: f32,
    ) -> bool {
        if !self.frames.contains_key(&frame_id) {
            tracing::debug!(
                frame_id,
                "child_frame_lifecycle: render_thread_child_remove_missing"
            );
            return false;
        }
        let removed = self.subtree_frame_ids(frame_id);
        let mut retired = Vec::new();
        for id in removed {
            let Some(entry) = self.frames.remove(&id) else {
                continue;
            };
            if let Some(animation) =
                EntryAnimation::start(spec, origin, slide_pixels, scale_from, true)
            {
                retired.push(DyingChildFrame { entry, animation });
            }
        }
        self.rebuild_presented_scene();
        tracing::info!(
            frame_id,
            dying = retired.len(),
            "child_frame_lifecycle: render_thread_child_retired"
        );
        self.dying.extend(retired);
        true
    }

    /// Drop dying frames whose close animation has finished at `sample`.
    ///
    /// Returns whether anything was removed: the caller must repaint once
    /// more, because the corpse's last drawn pixels are on screen and
    /// nothing else will ask for the frame that clears them.
    pub fn prune_dying(&mut self, sample: FrameSample) -> bool {
        let before = self.dying.len();
        self.dying
            .retain(|dying| !dying.animation.motion.sample(sample).finished);
        let pruned = self.dying.len() != before;
        if pruned {
            tracing::debug!(
                pruned = before - self.dying.len(),
                "child_frame_lifecycle: render_thread_dying_pruned"
            );
        }
        pruned
    }

    pub fn dying_entry(&self, frame_id: u64) -> Option<&DyingChildFrame> {
        self.dying
            .iter()
            .find(|dying| dying.entry.frame_id == frame_id)
    }

    pub fn has_dying(&self) -> bool {
        !self.dying.is_empty()
    }

    /// Attach a resize content crossfade to a freshly updated entry.
    ///
    /// Called after `update_frame` replaced the payload: the leased picture
    /// is the pre-update content, and the crossfade blends it into the new
    /// payload's picture over the resize slot's curve.
    pub fn begin_resize_crossfade(
        &mut self,
        frame_id: u64,
        old: SnapshotLease,
        old_width: f32,
        old_height: f32,
        spec: MotionSpec,
        origin: EventTime,
    ) {
        if let Some(entry) = self.frames.get_mut(&frame_id)
            && let Some(motion) = Motion::start(spec, origin)
        {
            entry.crossfade = Some(EntryCrossfade {
                motion,
                old,
                old_width,
                old_height,
            });
            tracing::info!(
                frame_id,
                old_width,
                old_height,
                "child_frame_lifecycle: resize_crossfade_started"
            );
        }
    }

    /// Start a placement drift for `frame_id`, departing from
    /// `from_x`/`from_y` toward the entry's placed position.
    ///
    /// Called when a re-anchoring update moved the popup and the movement
    /// slot is enabled. An in-flight drift is replaced; the fresh start
    /// departs from the last drawn position when the caller supplies one
    /// (see `retarget_drift`).
    pub fn begin_drift(
        &mut self,
        frame_id: u64,
        from_x: f32,
        from_y: f32,
        spec: MotionSpec,
        origin: EventTime,
    ) -> bool {
        let Some(motion) = Motion::start(spec, origin) else {
            return false;
        };
        if let Some(entry) = self.frames.get_mut(&frame_id) {
            entry.drift = Some(EntryDrift {
                motion,
                from_x,
                from_y,
                last_drawn: None,
            });
            tracing::info!(
                frame_id,
                from_x,
                from_y,
                "child_frame_lifecycle: drift_started"
            );
            true
        } else {
            false
        }
    }

    /// Retarget an in-flight drift from the position the previous pass
    /// painted, at the speed it had.
    ///
    /// A re-anchor arriving mid-drift must carry the popup on from where
    /// the user saw it, not restart it from a standstill at a place it
    /// never reached -- the same step-11 rule the pane morphs follow.
    pub fn retarget_drift(&mut self, frame_id: u64, spec: MotionSpec, origin: EventTime) -> bool {
        let Some(last) = self
            .frames
            .get(&frame_id)
            .and_then(|entry| entry.drift.as_ref())
            .and_then(|drift| drift.last_drawn)
        else {
            return false;
        };
        let Some(motion) = Motion::resume(spec, origin, ProgressRate::new(last.2)) else {
            return false;
        };
        if let Some(entry) = self.frames.get_mut(&frame_id) {
            entry.drift = Some(EntryDrift {
                motion,
                from_x: last.0,
                from_y: last.1,
                last_drawn: None,
            });
            true
        } else {
            false
        }
    }

    /// Clear finished drifts, sampled at `sample`. Returns whether anything
    /// was cleared, so the caller repaints once more.
    pub fn clear_finished_drifts(&mut self, sample: FrameSample) -> bool {
        let before = self
            .frames
            .values()
            .filter(|entry| entry.drift.is_some())
            .count();
        for entry in self.frames.values_mut() {
            if entry
                .drift
                .as_ref()
                .is_some_and(|drift| drift.motion.sample(sample).finished)
            {
                entry.drift = None;
            }
        }
        let after = self
            .frames
            .values()
            .filter(|entry| entry.drift.is_some())
            .count();
        before != after
    }

    /// Drop crossfades whose mix has finished at `sample`; the leases they
    /// held return to the pool. Returns whether anything was removed, so the
    /// caller repaints once more.
    pub fn prune_crossfades(&mut self, sample: FrameSample) -> bool {
        let before = self
            .frames
            .values()
            .filter(|entry| entry.crossfade.is_some())
            .count();
        for entry in self.frames.values_mut() {
            if entry
                .crossfade
                .as_ref()
                .is_some_and(|crossfade| crossfade.motion.sample(sample).finished)
            {
                entry.crossfade = None;
            }
        }
        let after = self
            .frames
            .values()
            .filter(|entry| entry.crossfade.is_some())
            .count();
        before != after
    }

    /// Reclaim time-expired composition ownership before mandatory admission.
    /// Sampling expiry is pure: do not advance drift, placement or the submitted
    /// interaction projection when a frame might still be refused.
    pub fn reclaim_finished_composition(&mut self, sample: FrameSample) -> bool {
        let mut changed = self.prune_crossfades(sample);
        changed |= self.prune_dying(sample);
        for entry in self.frames.values_mut() {
            if entry
                .animation
                .as_ref()
                .is_some_and(|animation| animation.motion.sample(sample).finished)
            {
                entry.animation = None;
                changed = true;
            }
        }
        changed
    }

    /// Drop every crossfade, returning their leases to the pool. The
    /// device-loss path calls this: the leased textures died with the
    /// device.
    pub fn drop_all_crossfades(&mut self) {
        for entry in self.frames.values_mut() {
            entry.crossfade = None;
        }
    }

    /// Whether any living entry carries a crossfade whose mix is unfinished.
    pub fn has_active_crossfade(&self, sample: FrameSample) -> bool {
        self.frames.values().any(|entry| {
            entry
                .crossfade
                .as_ref()
                .is_some_and(|crossfade| !crossfade.motion.sample(sample).finished)
        })
    }

    /// Record where a drifting frame was painted this pass, for the next
    /// re-anchor's retarget.
    pub fn record_drift_drawn(&mut self, frame_id: u64, x: f32, y: f32, rate: f32) {
        if let Some(entry) = self.frames.get_mut(&frame_id)
            && let Some(drift) = entry.drift.as_mut()
        {
            drift.last_drawn = Some((x, y, rate));
        }
    }

    /// Clear a finished lifecycle animation so `has_animation_activity`
    /// stops reporting it.
    pub fn clear_finished_animation(&mut self, frame_id: u64) {
        if let Some(entry) = self.frames.get_mut(&frame_id) {
            entry.animation = None;
        }
    }

    /// Whether any child frame is mid-animation, or a corpse is retained.
    ///
    /// Conservative by design: it reads animation *state*, not sampled
    /// progress, so it stays true until a scene pass clears a finished
    /// animation. The retained-static path consults it to stay ineligible —
    /// its texture is rebuilt only on a scene-generation change, so blitting
    /// it while a lifecycle animation runs would show the corpse at whatever
    /// alpha it had when the texture was last built.
    pub fn has_animation_activity(&self) -> bool {
        self.frames.values().any(|entry| {
            entry.animation.is_some() || entry.crossfade.is_some() || entry.drift.is_some()
        }) || self.has_dying()
    }

    /// Whether any child frame is mid-animation and needs another frame.
    pub fn has_active_animation(&self, sample: FrameSample) -> bool {
        self.frames.values().any(|entry| {
            entry
                .animation
                .as_ref()
                .is_some_and(|animation| !animation.motion.sample(sample).finished)
                || entry
                    .drift
                    .as_ref()
                    .is_some_and(|drift| !drift.motion.sample(sample).finished)
                || entry
                    .crossfade
                    .as_ref()
                    .is_some_and(|crossfade| !crossfade.motion.sample(sample).finished)
        }) || self
            .dying
            .iter()
            .any(|dying| !dying.animation.motion.sample(sample).finished)
    }

    /// Insert or update a child frame, recompute absolute position, rebuild render order.
    ///
    /// Returns true only when the rendered payload changed. Repeated delivery of
    /// an identical child-frame snapshot still refreshes liveness, but it must
    /// not look like a new frame install to face-cache and dirty-redraw logic.
    pub fn update_frame(&mut self, buf: FrameGlyphBuffer) -> bool {
        let frame_id = buf.frame_placement.frame();
        let outer = buf.frame_placement.outer_in_parent();
        let abs_x = outer.x();
        let abs_y = outer.y();
        let z_order = buf.frame_placement.z_order();
        // Taken before the payload below replaces the entry wholesale: an
        // in-flight appearance belongs to the popup, not to one payload.
        let previous_animation = self
            .frames
            .get(&frame_id.get())
            .and_then(|entry| entry.animation);
        // Same ownership as the appearance above: a drift belongs to the
        // popup across payload replaces. Taken through a mutable borrow so
        // the option moves out of the entry that is about to be replaced.
        let previous_drift = self
            .frames
            .get_mut(&frame_id.get())
            .and_then(|entry| entry.drift.take());
        let existing = self.frames.get_mut(&frame_id.get());

        // A re-delivery of a frame that is fading out cancels the fade: the
        // frame is alive again, and a close animation running underneath the
        // fresh entry would draw it toward gone.
        self.dying
            .retain(|dying| dying.entry.frame_id != frame_id.get());

        let glyph_count = buf.glyphs.len();
        if let Some(entry) = existing
            && entry.frame == buf
        {
            entry.last_updated = self.frame_counter;
            tracing::debug!(
                frame_id = frame_id.get(),
                abs_x,
                abs_y,
                width = buf.width,
                height = buf.height,
                z_order,
                glyphs = glyph_count,
                "child_frame_lifecycle: render_thread_child_buffer_unchanged"
            );
            return false;
        }

        let Ok(scene) = self.scene_with_replacement(&buf) else {
            tracing::error!(
                frame_id = frame_id.get(),
                "rejecting incoherent child-frame ancestry update"
            );
            return false;
        };
        let Ok(placed) = scene.place(PlaceChildQuery::new(
            buf.frame_placement.frame(),
            buf.frame_placement.presentation(),
        )) else {
            tracing::error!(
                frame_id = frame_id.get(),
                "rejecting child frame with invalid derived placement"
            );
            return false;
        };

        let existed = self.frames.contains_key(&frame_id.get());
        tracing::debug!(
            frame_id = frame_id.get(),
            abs_x,
            abs_y,
            width = buf.width,
            height = buf.height,
            z_order,
            glyphs = glyph_count,
            existed,
            "child_frame_lifecycle: render_thread_child_buffer"
        );

        let applied_frame_alpha = self
            .frames
            .get(&frame_id.get())
            .map_or(1.0, |entry| entry.applied_frame_alpha);
        self.frames.insert(
            frame_id.get(),
            ChildFrameEntry {
                frame_id: frame_id.get(),
                frame: buf,
                applied_frame_alpha,
                abs_x: placed.root_relative().x(),
                abs_y: placed.root_relative().y(),
                clip_in_root: placed.clip_in_root(),
                z_path: placed.z_path().to_vec(),
                last_updated: self.frame_counter,
                ingest_seq: super::frame_state::next_scene_generation(),
                // A payload refresh mid-appearance continues the animation
                // the previous payload started: the popup is still arriving,
                // and restarting its fade on every keystroke would read as
                // flicker.
                animation: previous_animation,
                // A payload replace starts its own crossfade story; any
                // in-flight one belongs to the replaced payload and drops
                // with it.
                crossfade: None,
                // A placement drift belongs to the popup, not to one
                // payload: the fresh entry settles at the same placed
                // position the drift was gliding toward.
                drift: previous_drift,
            },
        );

        self.apply_presented_scene(&scene);
        true
    }

    /// Remove a child frame by ID.
    pub fn remove_frame(&mut self, frame_id: u64) -> bool {
        if self.frames.contains_key(&frame_id) {
            let removed = self.subtree_frame_ids(frame_id);
            self.frames.retain(|id, _| !removed.contains(id));
            self.rebuild_presented_scene();
            tracing::info!(
                frame_id,
                "child_frame_lifecycle: render_thread_child_removed"
            );
            true
        } else {
            tracing::debug!(
                frame_id,
                "child_frame_lifecycle: render_thread_child_remove_missing"
            );
            false
        }
    }

    pub fn subtree_frame_ids(&self, frame_id: u64) -> std::collections::HashSet<u64> {
        let mut subtree = std::collections::HashSet::from([frame_id]);
        loop {
            let before = subtree.len();
            for (&id, entry) in &self.frames {
                if entry
                    .frame
                    .frame_placement
                    .parent()
                    .is_some_and(|parent| subtree.contains(&parent.get()))
                {
                    subtree.insert(id);
                }
            }
            if subtree.len() == before {
                return subtree;
            }
        }
    }

    pub fn subtree_presentations(&self, frame_id: u64) -> Vec<u64> {
        let mut presentations = self
            .subtree_frame_ids(frame_id)
            .into_iter()
            .filter_map(|id| self.frames.get(&id))
            .map(|entry| entry.frame.presentation_id.get())
            .filter(|presentation| *presentation != 0)
            .collect::<Vec<_>>();
        presentations.sort_unstable();
        presentations.dedup();
        presentations
    }

    /// Remove child frames not updated in the last `max_age` poll cycles.
    #[allow(dead_code)] // child-frame staleness API, exercised by the child_frames tests
    pub fn prune_stale(&mut self, max_age: u64) {
        let threshold = self.frame_counter.saturating_sub(max_age);
        let before = self.frames.len();
        self.frames
            .retain(|_, entry| entry.last_updated >= threshold);
        if self.frames.len() != before {
            self.rebuild_presented_scene();
        }
    }

    /// Get the z_order-sorted list of frame IDs for rendering.
    pub fn sorted_for_rendering(&self) -> &[u64] {
        &self.render_order
    }

    /// The merged draw order: every child frame — living and dying — in
    /// z-path order, a corpse drawing before a living frame at equal z.
    ///
    /// The corpse-first tie-break is the dismissal case: the dying popup is
    /// normally being replaced by the popup that took its place at the same
    /// z, and the old picture receding *beneath* the new one reads as the
    /// new one arriving. Interleaving matters for the rest: a dying child
    /// deep in the stack must not jump above an unrelated living sibling
    /// that happens to sit higher.
    pub fn merged_render_order(&self) -> Vec<(u64, bool)> {
        let mut order: Vec<(&[i32], u64, bool)> =
            Vec::with_capacity(self.frames.len() + self.dying.len());
        for (&id, entry) in &self.frames {
            order.push((entry.z_path.as_slice(), id, false));
        }
        for dying in &self.dying {
            order.push((dying.entry.z_path.as_slice(), dying.entry.frame_id, true));
        }
        order.sort_by(|a, b| {
            a.0.cmp(b.0).then(if a.2 == b.2 {
                a.1.cmp(&b.1)
            } else if a.2 {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            })
        });
        order
            .into_iter()
            .map(|(_, id, dying)| (id, dying))
            .collect()
    }

    /// Hit test: find the topmost child frame at the given point.
    /// Returns (frame_id: frame_id.get(), local_x, local_y) if hit, None otherwise.
    /// Iterates in reverse render order (topmost first).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<(u64, f32, f32)> {
        for &frame_id in self.render_order.iter().rev() {
            if let Some(entry) = self.frames.get(&frame_id) {
                let local_x = x - entry.abs_x;
                let local_y = y - entry.abs_y;
                if local_x >= 0.0
                    && local_y >= 0.0
                    && local_x < entry.frame.width
                    && local_y < entry.frame.height
                    && match entry.clip_in_root {
                        PresentedClip::Empty => false,
                        PresentedClip::Rect(clip) => {
                            x >= clip.x()
                                && y >= clip.y()
                                && x < clip.x() + clip.width()
                                && y < clip.y() + clip.height()
                        }
                    }
                {
                    return Some((frame_id, local_x, local_y));
                }
            }
        }
        None
    }

    /// Whether there are any child frames.
    #[allow(dead_code)] // exercised by the child_frames tests
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Rebuild the render order from z_order values.
    fn rebuild_render_order(&mut self) {
        self.render_order.clear();
        self.render_order.extend(self.frames.keys());
        // Sort by z_order ascending (lowest z = rendered first = behind)
        self.render_order.sort_by(|a, b| {
            let za = self
                .frames
                .get(a)
                .map(|e| e.z_path.as_slice())
                .unwrap_or(&[]);
            let zb = self
                .frames
                .get(b)
                .map(|e| e.z_path.as_slice())
                .unwrap_or(&[]);
            za.cmp(zb).then(a.cmp(b))
        });
    }

    fn rebuild_presented_scene(&mut self) {
        let mut placements = self.root.into_iter().collect::<Vec<_>>();
        placements.extend(
            self.frames
                .values()
                .map(|entry| entry.frame.frame_placement),
        );
        let Ok(scene) = PresentedFrameScene::from_placements(placements) else {
            tracing::error!("rejecting incoherent child-frame ancestry");
            return;
        };
        self.apply_presented_scene(&scene);
    }

    fn scene_with_replacement(
        &self,
        replacement: &FrameGlyphBuffer,
    ) -> Result<PresentedFrameScene, neomacs_display_protocol::PlaceChildError> {
        let replacement_id = replacement.frame_placement.frame().get();
        let mut placements = self.root.into_iter().collect::<Vec<_>>();
        placements.extend(
            self.frames
                .iter()
                .filter(|(id, _)| **id != replacement_id)
                .map(|(_, entry)| entry.frame.frame_placement),
        );
        placements.push(replacement.frame_placement);
        PresentedFrameScene::from_placements(placements)
    }

    fn apply_presented_scene(&mut self, scene: &PresentedFrameScene) {
        for entry in self.frames.values_mut() {
            let Ok(placed) = scene.place(PlaceChildQuery::new(
                entry.frame.frame_placement.frame(),
                entry.frame.frame_placement.presentation(),
            )) else {
                continue;
            };
            entry.abs_x = placed.root_relative().x();
            entry.abs_y = placed.root_relative().y();
            entry.clip_in_root = placed.clip_in_root();
            entry.z_path = placed.z_path().to_vec();
        }
        self.rebuild_render_order();
    }
}

#[cfg(test)]
#[path = "child_frames/tests/child_frames_test.rs"]
mod tests;
