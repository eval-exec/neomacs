//! Native resize completion, whether returned synchronously or delivered later.

use super::RenderApp;
use super::state::emacs_pixels_from_window_size;
use crate::thread_comm::{InputEvent, RenderComms};
use neovm_core::window::GuiResizeRequestId;
use winit::{dpi::PhysicalSize, window::WindowId};

/// A request is not a completion. In particular, winit's immediate result may
/// be the old size if the platform rejected the request, and need not produce
/// a later event. Never substitute the requested dimensions for this result.
#[must_use]
pub(super) enum ResizeRequestOutcome {
    Applied {
        window: WindowId,
        size: PhysicalSize<u32>,
        resize_request_id: Option<GuiResizeRequestId>,
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
            ResizeRequestOutcome::Applied {
                window,
                size,
                resize_request_id,
            } => {
                self.apply_native_surface_resize_with_completion(window, size, resize_request_id);
            }
            ResizeRequestOutcome::AwaitingConfigure | ResizeRequestOutcome::PendingRealization => {}
        }
    }

    pub(super) fn apply_native_surface_resize(
        &mut self,
        window: WindowId,
        size: PhysicalSize<u32>,
    ) {
        self.apply_native_surface_resize_with_completion(window, size, None);
    }

    fn apply_native_surface_resize_with_completion(
        &mut self,
        window: WindowId,
        size: PhysicalSize<u32>,
        resize_request_id: Option<GuiResizeRequestId>,
    ) {
        let emacs_fid = self
            .frame_windows
            .event_frame_for_winit(window)
            .unwrap_or(0);
        let is_primary = self.frame_windows.is_primary_winit(window);
        if let Some(gpu) = self.gpu.as_ref()
            && let Some(ws) = self.frame_windows.get_by_winit_mut(window)
        {
            ws.handle_resize(&gpu.device, &gpu.instance, size.width, size.height);
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
            }
            let scale_factor = ws.scale_factor();
            let (content_width, content_height) = ws.content_size();
            let (width, height) =
                emacs_pixels_from_window_size(content_width, content_height, scale_factor);
            // The surface already has the new size, but the retained scene
            // was laid out for the old one. GNU queues the native size in the
            // resize handler (gtkutil.c:1159, delay=true) and the next
            // redisplay applies it before drawing (dispnew.c:6694-6742), so
            // it never draws the pre-resize layout on the resized surface.
            // The evaluator's relaid presentation arrives asynchronously
            // here; withhold the superseded extent until it does.
            ws.render.note_native_resize(width, height);
            publish_native_resize(
                &self.comms,
                width,
                height,
                scale_factor,
                emacs_fid,
                resize_request_id,
            );
        }
    }
}

/// Native allocation observations remain authoritative independently of the
/// requested grid. A terminal reply retires only its own pending request,
/// after the evaluator has received the actual allocation, even on rejection.
fn publish_native_resize(
    comms: &RenderComms,
    width: u32,
    height: u32,
    scale_factor: f64,
    emacs_frame_id: u64,
    resize_request_id: Option<GuiResizeRequestId>,
) {
    comms.send_input(InputEvent::WindowResize {
        width,
        height,
        scale_factor,
        emacs_frame_id,
    });
    if let Some(request_id) = resize_request_id {
        comms.send_input(InputEvent::WindowResizeRequestCompleted {
            request_id,
            emacs_frame_id,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread_comm::ThreadComms;

    #[test]
    fn terminal_resize_reply_follows_actual_allocation_and_preserves_each_serial() {
        let (emacs, render) = ThreadComms::new().split();
        // Identical rejected allocations, distinct requests, then a duplicate
        // completion. Neither actual pixels nor equality can identify intent.
        for serial in [41, 42, 42] {
            let expected_id = GuiResizeRequestId::new(serial).unwrap();
            publish_native_resize(&render, 1920, 1080, 2.0, 99, Some(expected_id));
            assert!(matches!(
                emacs.input_rx.try_recv().unwrap(),
                InputEvent::WindowResize {
                    width: 1920,
                    height: 1080,
                    scale_factor,
                    emacs_frame_id: 99,
                } if scale_factor == 2.0
            ));
            match emacs.input_rx.try_recv().unwrap() {
                InputEvent::WindowResizeRequestCompleted {
                    request_id,
                    emacs_frame_id,
                } => {
                    assert_eq!(request_id, expected_id);
                    assert_eq!(emacs_frame_id, 99);
                }
                other => panic!("expected terminal resize reply, got {other:?}"),
            }
            assert!(emacs.input_rx.try_recv().is_err());
        }
    }

    #[test]
    fn unsolicited_native_resize_publishes_no_terminal_request_reply() {
        let (emacs, render) = ThreadComms::new().split();
        publish_native_resize(&render, 942, 780, 2.0, 99, None);
        assert!(matches!(
            emacs.input_rx.try_recv().unwrap(),
            InputEvent::WindowResize {
                width: 942,
                height: 780,
                emacs_frame_id: 99,
                ..
            }
        ));
        assert!(emacs.input_rx.try_recv().is_err());
    }
}
