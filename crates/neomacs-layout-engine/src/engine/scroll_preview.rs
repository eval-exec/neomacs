//! Publish a canonical command destination against existing certified rows.
use super::*;
use neomacs_display_protocol::input_progress::InputReceipt;
use neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent;
use neovm_core::emacs_core::neo::scroll::ScrollContextExt;
use neovm_core::window::DisplayFrameExt;
use neovm_core::window::DisplayFrameManagerExt;
use neovm_core::window::display::WindowDisplayContextExt;
use neovm_core::window::{FrameId, WindowId};

/// Evaluator-owned semantic rows follow the same fresh/renew/revoke protocol
/// as the renderer's retained surface. Renewal-only publications are deltas,
/// not complete snapshots of the rows available for command destinations.
#[derive(Default)]
pub(super) struct ScrollPreviewCoverage {
    frame: Option<neomacs_display_protocol::DisplayFrameId>,
    entries: rustc_hash::FxHashMap<DisplayWindowId, CertifiedPreviewCoverage>,
}

struct CertifiedPreviewCoverage {
    coverage: std::sync::Arc<neomacs_display_protocol::scroll_coverage::ScrollCoverage>,
    certified_at: neomacs_display_protocol::PresentationId,
    compositor_enabled: bool,
}

impl ScrollPreviewCoverage {
    pub(super) fn clear(&mut self) {
        self.frame = None;
        self.entries.clear();
    }

    /// Commit only accepted, sealed publications. A failed layout attempt
    /// cannot revoke the rows belonging to the last accepted presentation.
    pub(super) fn refresh(&mut self, frame: &neomacs_display_protocol::SealedFramePresentation) {
        let owner = frame.frame_placement.frame();
        if self.frame != Some(owner) {
            self.clear();
            self.frame = Some(owner);
        }
        let presentation = frame.presentation();
        for coverage in &frame.scroll_coverage {
            self.entries.insert(
                coverage.content.window_id,
                CertifiedPreviewCoverage {
                    coverage: std::sync::Arc::clone(coverage),
                    certified_at: presentation,
                    compositor_enabled: coverage.compositor_enabled,
                },
            );
        }
        for renewal in &frame.scroll_coverage_renewals {
            match self.entries.get_mut(&renewal.window) {
                Some(entry) if entry.coverage.epoch == renewal.epoch => {
                    entry.certified_at = presentation;
                    entry.compositor_enabled = renewal.compositor_enabled;
                }
                Some(_) => {
                    self.entries.remove(&renewal.window);
                }
                None => {}
            }
        }
        // Neither a fresh export nor a matching renewal certified these rows
        // for this publication. This also prunes deleted windows.
        self.entries
            .retain(|_, entry| entry.certified_at == presentation);
    }

    fn get(&self, frame: FrameId, window: DisplayWindowId) -> Option<&CertifiedPreviewCoverage> {
        if self.frame != Some(neomacs_display_protocol::DisplayFrameId::new(frame.0)) {
            return None;
        }
        self.entries.get(&window)
    }
}

impl LayoutEngine {
    pub fn resolved_scroll_preview(
        &self,
        evaluator: &neovm_core::emacs_core::Context,
        frame: FrameId,
        window: WindowId,
        inputs: Vec<InputReceipt>,
    ) -> Option<ResolvedScrollIntent> {
        if inputs.is_empty()
            || inputs.len() > 128
            || self.retained_frame != Some(frame)
            || !evaluator.compositor_scrolling_enabled(window)
        {
            return None;
        }
        let owner = DisplayWindowId::new(window.0 as i64);
        let retained_coverage = self.scroll_preview_coverage.get(frame, owner)?;
        if !retained_coverage.compositor_enabled {
            return None;
        }
        let coverage = &retained_coverage.coverage;
        let retained = self.retained_window_matrices.get(&owner)?;
        let live = evaluator
            .frame_manager()
            .get(frame)?
            .find_window(window)?
            .redisplay_state()?;
        let before = retained.display_snapshot.layout_freshness.as_ref()?;
        let after = evaluator.window_display_snapshot_freshness(frame, window, live.buffer_id)?;
        if !before.same_scroll_content(&after) {
            tracing::debug!(target: "neomacs_layout_engine::scroll_coverage", "resolved scroll source changed");
            return None;
        }
        let start = live.window_start.as_i64().checked_sub(1)? as usize;
        let row = coverage
            .content
            .matrix
            .rows
            .iter()
            .find(|row| row.enabled && row.start_charpos == start)?;
        let offset = coverage.content.text_pixel_bounds.y + row.pixel_y
            - coverage.origin
            - coverage.viewport.y
            + live.vscroll.saturating_neg().max(0) as f32;
        if !offset.is_finite() {
            return None;
        }
        Some(ResolvedScrollIntent {
            frame: frame.0,
            window: owner,
            presentation: coverage.hit_index.presentation(),
            epoch: coverage.epoch,
            publication: coverage.publication,
            offset,
            inputs,
        })
    }
}

#[cfg(test)]
#[path = "scroll_preview/tests/retention_test.rs"]
mod tests;
