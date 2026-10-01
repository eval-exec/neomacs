//! Publish a canonical command destination against existing certified rows.
use super::*;
use neomacs_display_protocol::input_progress::InputReceipt;
use neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent;
use neovm_core::window::{FrameId, WindowId};

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
        let coverage = self
            .scroll_preview_coverage
            .iter()
            .find(|coverage| coverage.content.window_id == owner)?;
        if !coverage.compositor_enabled {
            return None;
        }
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
            offset,
            inputs,
        })
    }
}
