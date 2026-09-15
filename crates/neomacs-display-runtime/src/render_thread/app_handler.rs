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
                    .send_input(crate::thread_comm::InputEvent::close_requested(0));
                self.lifecycle_flags.shutdown_requested = true;
                self.handle_exiting();
                event_loop.exit();
            }
            // Read current size/scale from the native window upon completion.
            return;
        }
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
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Waking is enough: the real about_to_wait drains commands after all
        // pending native events. Running that pass here creates/destroys popup
        // surfaces in the middle of winit's dispatch iteration.
    }
}

impl Drop for RenderApp {
    fn drop(&mut self) {
        self.menus.close();
        self.handle_exiting();
    }
}
