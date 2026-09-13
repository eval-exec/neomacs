//! Handing one drawn frame to the platform, and publishing what it says about
//! the screen.
//!
//! Owns: the entry point the frame coordinator calls, the pre-present pacing
//! notification, the `queue().present` itself, the debug surface readbacks,
//! the child-frame lifecycle tracing around the present, and the one place the
//! interaction projection becomes visible.
//!
//! Must not: draw. Everything here runs after the draw order has either
//! produced a [`RenderedFrameSurface`] or given up with a typed failure, and
//! the split is load-bearing: the projection is published *after submission*,
//! never for a render attempt abandoned by an early return. Queue submission
//! is not proof of display. Input deliberately uses the latest submitted
//! projection without waiting for optional compositor confirmation.

use super::{RenderApp, RenderedFrameSurface};
use crate::render_thread::frame_sched::SubmissionResult;
use crate::render_thread::{frame_stats, surface_readback};

impl RenderApp {
    /// Render and present one top-level frame window, preserving the precise
    /// outcome for the frame coordinator.
    ///
    /// `compositor_only_hint` is set when the frame coordinator's plan is
    /// compositor-only; it enables the retained-static fast path (blit the
    /// retained scene, sample dynamic cursor/post state) when the scene is
    /// eligible, skipping the glyph pipeline.
    pub(in crate::render_thread) fn render_frame_window_hinted(
        &mut self,
        emacs_frame_id: u64,
        compositor_only_hint: bool,
    ) -> SubmissionResult {
        self.render_frame_window_impl(emacs_frame_id, compositor_only_hint)
    }

    fn render_frame_window_impl(
        &mut self,
        emacs_frame_id: u64,
        compositor_only_hint: bool,
    ) -> SubmissionResult {
        if self.lifecycle_flags.shutdown_requested {
            return SubmissionResult::Skipped;
        }
        self.prepare_frame_state_for_render();

        let bg_gradient = if self.effects.bg_gradient.enabled {
            Some((
                self.effects.bg_gradient.top,
                self.effects.bg_gradient.bottom,
            ))
        } else {
            None
        };

        let is_primary_frame = self.frame_windows.is_primary_frame_id(emacs_frame_id);
        let Some(renderer) = self.renderer.as_mut() else {
            return SubmissionResult::Timeout;
        };
        let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) else {
            return SubmissionResult::Timeout;
        };
        if let Some((width, height)) = window_state.synchronize_window_chrome() {
            let (width, height) = crate::render_thread::state::emacs_pixels_from_window_size(
                width,
                height,
                window_state.scale_factor(),
            );
            self.comms
                .send_input(crate::thread_comm::InputEvent::WindowResize {
                    emacs_frame_id,
                    width,
                    height,
                    scale_factor: window_state.scale_factor(),
                });
        }
        #[cfg(feature = "video")]
        renderer.begin_video_surface_render();
        window_state
            .render
            .compositor
            .transitions
            .apply_policy(self.transition_policy);

        let rendered = super::render_frame_window_contents_to_surface(
            renderer,
            window_state,
            bg_gradient,
            &self.child_frame_style,
            self.scroll_indicators_enabled,
            &self.toolbar,
            self.extra_line_spacing,
            self.extra_letter_spacing,
            compositor_only_hint,
            &self.render_policy,
            &mut self.device_lost,
        );
        let RenderedFrameSurface {
            output,
            frame,
            projection,
        } = match rendered {
            Ok(rendered) => rendered,
            Err(failure) => {
                #[cfg(feature = "video")]
                renderer.cancel_video_surface_render();
                return failure.submission_result();
            }
        };
        if is_primary_frame {
            let (w, h) = self
                .frame_windows
                .get(emacs_frame_id)
                .map(|ws| ws.native_size())
                .unwrap_or((0, 0));
            surface_readback::maybe_log_first_frame_surface_readback(
                &mut self.debug_first_frame_readback_pending,
                &output.texture,
                renderer,
                &frame,
                w,
                h,
            );
            surface_readback::maybe_log_debug_surface_readback(
                &mut self.debug_surface_readback_frames_remaining,
                &output.texture,
                renderer,
                &frame,
                w,
                h,
            );
        }
        let (child_frame_ids, removed_child_frame_ids) = self
            .frame_windows
            .get_mut(emacs_frame_id)
            .map(|window_state| {
                let child_frame_ids = window_state
                    .render
                    .compositor
                    .child_frames
                    .sorted_for_rendering()
                    .to_vec();
                let removed_child_frame_ids = std::mem::take(
                    &mut window_state
                        .render
                        .compositor
                        .pending_child_frame_removals_to_present,
                );
                (child_frame_ids, removed_child_frame_ids)
            })
            .unwrap_or_default();
        if !child_frame_ids.is_empty() || !removed_child_frame_ids.is_empty() {
            tracing::debug!(
                parent_frame_id = emacs_frame_id,
                child_frame_ids = ?child_frame_ids,
                removed_child_frame_ids = ?removed_child_frame_ids,
                "child_frame_lifecycle: present_begin"
            );
        }
        // Let winit arm platform pacing (the Wayland surface frame
        // callback) for the upcoming present; a no-op elsewhere.
        if let Some(window_state) = self.frame_windows.get(emacs_frame_id)
            && let Some(window) = window_state.window()
        {
            let (width, height) = window_state.content_size();
            let logical_size = crate::render_thread::state::emacs_pixels_from_window_size(
                width,
                height,
                window_state.scale_factor(),
            );
            self.presentation_observer.before_present(
                window.as_ref(),
                emacs_frame_id,
                logical_size,
                window_state.scale_factor(),
            );
            window.pre_present_notify();
        }
        renderer.queue().present(output);
        // This projection describes the frame just handed to the compositor.
        // Keep input publication at submission; native confirmation is separate.
        if let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) {
            window_state.render.publish_submitted_projection(projection);
        }
        #[cfg(feature = "video")]
        renderer.finish_presented_video_surface();
        frame_stats::note_present(neomacs_display_protocol::frame_time::observe_platform_now());

        if !child_frame_ids.is_empty() || !removed_child_frame_ids.is_empty() {
            tracing::debug!(
                parent_frame_id = emacs_frame_id,
                child_frame_ids = ?child_frame_ids,
                removed_child_frame_ids = ?removed_child_frame_ids,
                "child_frame_lifecycle: present_done"
            );
        }
        SubmissionResult::Submitted
    }
}
