//! Certified scroll coverage retained across publishes that carry no surface.
//!
//! Every publish either exports a fresh coverage surface, renews a certified
//! epoch (the rows are unchanged, so re-exporting would only repeat work), or
//! says nothing about a window — and silence is revocation: the publish saw a
//! content revision the retained coverage does not describe. Reading surface
//! presence alone cannot distinguish the first two from the third, which is
//! exactly what a cursor-only redisplay between two scrolls used to do: the
//! presentation advanced, the surface vanished, and input-driven scrolling
//! stalled until the offscreen worker exported again.
//!
//! This map keeps, per window, the last certified surface plus the newest
//! presentation that certified it. [`CoverageRetention::refresh`] applies one
//! publish's surfaces and renewals with those three outcomes — install,
//! revalidate, expire — and the lookups answer "is this epoch certified for
//! exactly this presentation".

use neomacs_display_protocol::scroll_coverage::{
    AuxiliaryScrollCoverage, CoveragePublicationId, ScrollSurface,
};
use neomacs_display_protocol::types::DisplayWindowId;
use neomacs_display_protocol::{FrameGlyphBuffer, PresentationId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Default)]
pub(in crate::render_thread) struct CoverageRetention {
    entries: HashMap<DisplayWindowId, RetainedCoverage>,
}

pub(in crate::render_thread) struct RetainedCoverage {
    surface: Arc<ScrollSurface>,
    /// The newest publish that certified this coverage: the fresh surface's
    /// own frame, or the frame of the latest renewal.
    valid_presentation: PresentationId,
    /// Policy as stamped by the certifying publish. A renewal may flip either
    /// bit without touching the rows, so they are kept beside the surface
    /// instead of read from it.
    predict_pixels: bool,
    compositor_enabled: bool,
}

impl RetainedCoverage {
    pub(in crate::render_thread) fn surface(&self) -> &Arc<ScrollSurface> {
        &self.surface
    }

    pub(in crate::render_thread) fn predict_pixels(&self) -> bool {
        self.predict_pixels
    }

    pub(in crate::render_thread) fn compositor_enabled(&self) -> bool {
        self.compositor_enabled
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::render_thread) struct AuxiliaryCoverageChanges {
    pub installed: usize,
    pub revoked: usize,
    /// Absent windows revoked by a snapshot for a different logical base.
    pub conservative_revocations: usize,
}

impl AuxiliaryCoverageChanges {
    pub fn changed(&self) -> bool {
        self.installed != 0 || self.revoked != 0
    }
}

/// Shared render-thread admission check for both primary (`refresh`) and
/// auxiliary (`apply_auxiliary`) compositor coverage surfaces.
fn surface_compatible_with_frame(surface: &ScrollSurface, frame: &FrameGlyphBuffer) -> bool {
    let coverage = surface.coverage();
    coverage.hit_index.presentation() == frame.presentation_id
        && surface.frame_buffer_resources_match(frame)
        && coverage.resources_compatible_with_frame_buffer(frame)
}

impl CoverageRetention {
    /// Apply a complete desired snapshot. Revocation is intentionally more
    /// conservative than installation: an absent window loses coverage even
    /// when the native frame has already advanced past this update's base.
    pub(in crate::render_thread) fn apply_auxiliary(
        &mut self,
        update: AuxiliaryScrollCoverage,
        frame: Option<&FrameGlyphBuffer>,
    ) -> AuxiliaryCoverageChanges {
        let desired: HashSet<_> = update
            .surfaces
            .iter()
            .map(|surface| surface.coverage().content.window_id)
            .collect();
        let mut changes = AuxiliaryCoverageChanges::default();
        self.entries.retain(|window, _| {
            if desired.contains(window) {
                true
            } else {
                changes.revoked += 1;
                false
            }
        });
        let Some(frame) = frame.filter(|frame| {
            frame.presentation_id == update.base && frame.frame_placement.frame() == update.frame
        }) else {
            if !update.surfaces.is_empty() {
                tracing::debug!(
                    target: "neomacs_display_runtime::retained_scroll",
                    update_base = update.base.get(),
                    update_frame = update.frame.get(),
                    current_base = frame.map(|frame| frame.presentation_id.get()),
                    current_frame = frame.map(|frame| frame.frame_placement.frame().get()),
                    "rejected auxiliary compositor coverage base mismatch"
                );
            }
            changes.conservative_revocations = changes.revoked;
            return changes;
        };
        for surface in update.surfaces {
            let coverage = surface.coverage();
            let window = coverage.content.window_id;
            if !surface_compatible_with_frame(&surface, frame) {
                tracing::debug!(
                    target: "neomacs_display_runtime::retained_scroll",
                    window = window.get(),
                    base = update.base.get(),
                    hit_index_base = coverage.hit_index.presentation().get(),
                    "rejected auxiliary compositor coverage surface"
                );
                changes.revoked += usize::from(self.entries.remove(&window).is_some());
                continue;
            }
            if self.entries.get(&window).is_some_and(|entry| {
                Arc::ptr_eq(&entry.surface, &surface)
                    && entry.valid_presentation == update.base
                    && entry.predict_pixels == coverage.predict_pixels
                    && entry.compositor_enabled == coverage.compositor_enabled
            }) {
                continue;
            }
            let predict_pixels = coverage.predict_pixels;
            let compositor_enabled = coverage.compositor_enabled;
            let epoch = coverage.epoch;
            let publication = coverage.publication.get();
            let anchor_row = coverage.anchor_row;
            let origin = coverage.origin;
            self.entries.insert(
                window,
                RetainedCoverage {
                    predict_pixels,
                    compositor_enabled,
                    surface,
                    valid_presentation: update.base,
                },
            );
            tracing::debug!(
                target: "neomacs_display_runtime::retained_scroll",
                window = window.get(),
                base = update.base.get(),
                epoch,
                publication,
                compositor_enabled,
                predict_pixels,
                anchor_row,
                origin,
                "installed auxiliary compositor coverage"
            );
            changes.installed += 1;
        }
        changes
    }

    /// Apply one ingested publish. A window absent from both its surfaces and
    /// its renewals loses its entry: that publish either revoked the coverage
    /// or coalesced away the publish that carried it, and a surface of unknown
    /// freshness must never be painted or projected.
    pub(in crate::render_thread) fn refresh(&mut self, frame: &FrameGlyphBuffer) {
        let mut certified: HashSet<DisplayWindowId> = frame
            .scroll_surfaces
            .iter()
            .map(|surface| surface.coverage().content.window_id)
            .collect();
        certified.extend(
            frame
                .scroll_coverage_renewals
                .iter()
                .map(|renewal| renewal.window),
        );
        self.entries.retain(|window, _| certified.contains(window));
        for surface in &frame.scroll_surfaces {
            let coverage = surface.coverage();
            let window = coverage.content.window_id;
            if !surface_compatible_with_frame(surface, frame) {
                tracing::debug!(
                    target: "neomacs_display_runtime::retained_scroll",
                    window = window.get(),
                    base = frame.presentation_id.get(),
                    hit_index_base = coverage.hit_index.presentation().get(),
                    "rejected primary compositor coverage surface"
                );
                self.entries.remove(&window);
                continue;
            }
            self.entries.insert(
                window,
                RetainedCoverage {
                    surface: Arc::clone(surface),
                    valid_presentation: frame.presentation_id,
                    predict_pixels: coverage.predict_pixels,
                    compositor_enabled: coverage.compositor_enabled,
                },
            );
        }
        for renewal in &frame.scroll_coverage_renewals {
            let Some(entry) = self.entries.get(&renewal.window) else {
                continue;
            };
            if entry.surface.coverage().epoch != renewal.epoch {
                // The evaluator moved on to an epoch this compositor never
                // received; the one it holds describes revoked rows.
                self.entries.remove(&renewal.window);
                continue;
            }
            let entry = self.entries.get_mut(&renewal.window).expect("just checked");
            entry.valid_presentation = frame.presentation_id;
            entry.predict_pixels = renewal.predict_pixels;
            entry.compositor_enabled = renewal.compositor_enabled;
        }
    }

    /// Coverage certified for exactly this presentation, any epoch.
    pub(in crate::render_thread) fn certified_at(
        &self,
        presentation: PresentationId,
        window: DisplayWindowId,
    ) -> Option<&RetainedCoverage> {
        self.entries
            .get(&window)
            .filter(|entry| entry.valid_presentation == presentation)
    }

    /// Coverage certified for exactly this presentation and epoch.
    pub(in crate::render_thread) fn certified_epoch_at(
        &self,
        presentation: PresentationId,
        window: DisplayWindowId,
        epoch: u64,
    ) -> Option<&RetainedCoverage> {
        self.certified_at(presentation, window)
            .filter(|entry| entry.surface.coverage().epoch == epoch)
    }

    /// An offset belongs to one exact export, not merely its content lease.
    /// Renewals keep that retained export's identity; a same-epoch replacement
    /// must not inherit permission to use offsets chosen against its predecessor.
    pub(in crate::render_thread) fn certified_publication_at(
        &self,
        presentation: PresentationId,
        window: DisplayWindowId,
        epoch: u64,
        publication: CoveragePublicationId,
    ) -> Option<&RetainedCoverage> {
        self.certified_epoch_at(presentation, window, epoch)
            .filter(|entry| entry.surface.coverage().publication == publication)
    }

    /// Every entry certified for exactly this presentation, in map order.
    /// Pointer prediction uses this to find the window under a gesture.
    pub(in crate::render_thread) fn certified_windows_at(
        &self,
        presentation: PresentationId,
    ) -> impl Iterator<Item = &RetainedCoverage> {
        self.entries
            .values()
            .filter(move |entry| entry.valid_presentation == presentation)
    }
}

#[cfg(test)]
#[path = "coverage_retention/tests/coverage_retention_test.rs"]
mod tests;
