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
        self.handle_window_event(event_loop, window_id, event);
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
