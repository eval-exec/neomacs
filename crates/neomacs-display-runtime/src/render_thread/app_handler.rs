use super::RenderApp;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

impl ApplicationHandler for RenderApp {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.handle_resumed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .gpu_startup
            .as_ref()
            .is_some_and(|pending| pending.window_id() == window_id)
        {
            if let WindowEvent::SurfaceResized(size) = &event {
                self.observe_pending_content(*size);
            }
            if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                self.comms
                    .send_input(crate::thread_comm::InputEvent::WindowClose { emacs_frame_id: 0 });
                self.lifecycle_flags
                    .request_shutdown(super::state::RenderShutdownReason::StartupCancelled);
                self.handle_exiting();
                event_loop.exit();
                return;
            }
            // GPU initialization must not consume native input. Keep its
            // modifier, focus and IME ordering until this window has an owner.
            self.gpu_startup.as_mut().unwrap().retain_input(event);
            return;
        }
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed)
            && self
                .startup_input
                .as_ref()
                .is_some_and(|input| input.window_id() == window_id)
        {
            self.startup_input = None;
        }
        let event = if let Some(input) = self
            .startup_input
            .as_mut()
            .filter(|input| input.window_id() == window_id)
        {
            match input.defer_input(event) {
                Ok(()) => return,
                Err(event) => event,
            }
        } else {
            event
        };
        self.handle_window_event(event_loop, window_id, event);
    }

    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Workers own no native surface. Dropping a pending attempt destroys
        // its surface synchronously and rejects every later worker result.
        // A subsequent can_create_surfaces starts a fresh attempt.
        self.cancel_gpu_startup();
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.handle_about_to_wait(event_loop);
        flush_native_display_requests(event_loop);
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Waking is enough: the real about_to_wait drains commands after all
        // pending native events. Running that pass here creates/destroys popup
        // surfaces in the middle of winit's dispatch iteration.
    }
}

/// Commit native requests issued by this batch before the event loop sleeps.
/// Winit's X11 window drop queues DestroyWindow without flushing it. Its
/// separate event-reader thread can already be blocked in XPeekEvent, leaving
/// the last destroyed window visible until some unrelated request flushes.
fn flush_native_display_requests(event_loop: &dyn ActiveEventLoop) {
    #[cfg(target_os = "linux")]
    {
        use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
        use std::sync::OnceLock;
        use x11_dl::xlib;

        let Ok(display) = event_loop.display_handle() else {
            return;
        };
        let RawDisplayHandle::Xlib(display) = display.as_raw() else {
            return;
        };
        let Some(display) = display.display else {
            return;
        };
        static XLIB: OnceLock<Option<xlib::Xlib>> = OnceLock::new();
        let Some(xlib) = XLIB
            .get_or_init(|| match xlib::Xlib::open() {
                Ok(xlib) => Some(xlib),
                Err(error) => {
                    tracing::warn!(%error, "cannot flush native X11 display requests");
                    None
                }
            })
            .as_ref()
        else {
            return;
        };
        // SAFETY: the active event loop owns this live Xlib display, and this
        // callback runs on its native owner thread after window/surface drops.
        // XFlush sends pending requests without waiting for server responses.
        unsafe { (xlib.XFlush)(display.as_ptr().cast()) };
    }
    #[cfg(not(target_os = "linux"))]
    let _ = event_loop;
}

impl Drop for RenderApp {
    fn drop(&mut self) {
        self.menus.close();
        self.handle_exiting();
    }
}
