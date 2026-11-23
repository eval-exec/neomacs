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

use neomacs_display_protocol::scroll_coverage::{CoveragePublicationId, ScrollSurface};
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

impl CoverageRetention {
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
            self.entries.insert(
                coverage.content.window_id,
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
