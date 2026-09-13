//! Native resize completion, whether returned synchronously or delivered later.

use super::RenderApp;
use super::state::emacs_pixels_from_window_size;
use crate::thread_comm::InputEvent;
use winit::{dpi::PhysicalSize, window::WindowId};

/// A request is not a completion. In particular, winit's immediate result may
/// be the old size if the platform rejected the request, and need not produce
/// a later event. Never substitute the requested dimensions for this result.
#[must_use]
pub(super) enum ResizeRequestOutcome {
    Applied {
        window: WindowId,
        size: PhysicalSize<u32>,
    },
    AwaitingConfigure,
    PendingRealization,
}

impl RenderApp {
    /// Some backends deliver scale changes without a resize event. Once the
    /// callback has returned, sample the final native size before rendering or
    /// waiting. Never reinterpret our cached old size with the new scale.
    pub(super) fn complete_pending_scale_change(&mut self, window: WindowId) {
        let size = self.frame_windows.get_by_winit(window).and_then(|ws| {
            ws.pending_scale_factor?;
            ws.window().map(|window| window.surface_size())
        });
        if let Some(size) = size {
            self.apply_native_surface_resize(window, size);
        }
    }

    pub(super) fn complete_pending_scale_changes(&mut self) {
        let windows: Vec<_> = self
            .frame_windows
            .windows
            .values()
            .filter_map(|ws| {
                ws.pending_scale_factor?;
                ws.window().map(|window| window.id())
            })
            .collect();
        for window in windows {
            self.complete_pending_scale_change(window);
        }
    }

    pub(super) fn complete_resize_request(&mut self, outcome: ResizeRequestOutcome) {
        match outcome {
            ResizeRequestOutcome::Applied { window, size } => {
                self.apply_native_surface_resize(window, size);
            }
            ResizeRequestOutcome::AwaitingConfigure | ResizeRequestOutcome::PendingRealization => {}
        }
    }

    pub(super) fn apply_native_surface_resize(
        &mut self,
        window: WindowId,
        size: PhysicalSize<u32>,
    ) {
        let emacs_fid = self
            .frame_windows
            .event_frame_for_winit(window)
            .unwrap_or(0);
        let is_primary = self.frame_windows.is_primary_winit(window);
        if let Some(device) = self.gpu.as_ref().map(|gpu| gpu.device.clone())
            && let Some(ws) = self.frame_windows.get_by_winit_mut(window)
        {
            ws.handle_resize(&device, size.width, size.height);
            if is_primary {
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_scale_factor(ws.scale_factor() as f32);
                    renderer.resize(size.width, size.height);
                }
                if self.effects.resize_padding.enabled
                    && let Some(renderer) = self.renderer.as_ref()
                {
                    renderer.trigger_transient_resize_padding(
                        &mut ws.render.compositor.renderer_effects,
                        neomacs_display_protocol::frame_time::observe_platform_now().into_instant(),
                    );
                }
                ws.render.mark_dirty();
            }
            let scale_factor = ws.scale_factor();
            let (content_width, content_height) = ws.content_size();
            let (width, height) =
                emacs_pixels_from_window_size(content_width, content_height, scale_factor);
            self.comms.send_input(InputEvent::WindowResize {
                width,
                height,
                scale_factor,
                emacs_frame_id: emacs_fid,
            });
        }
    }
}
