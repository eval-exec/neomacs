use super::{RenderApp, RenderUserEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

impl ApplicationHandler<RenderUserEvent> for RenderApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.handle_resumed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.handle_window_event(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.handle_about_to_wait(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: RenderUserEvent) {
        match event {
            RenderUserEvent::Wake => self.handle_about_to_wait(event_loop),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.handle_exiting();
    }
}
