//! Browser canvas and event-loop adapter.

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use wasm_bindgen::prelude::*;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowId};

struct BrowserFrontend {
    lifecycle: FrontendLifecycle,
    window: Option<Window>,
}

impl Default for BrowserFrontend {
    fn default() -> Self {
        Self {
            lifecycle: FrontendLifecycle::new(),
            window: None,
        }
    }
}

impl ApplicationHandler for BrowserFrontend {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.lifecycle.transition(LifecycleEvent::Resumed) != LifecycleAction::CreateFrontend {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Neomacs")
            .with_append(true)
            .with_focusable(true)
            .with_prevent_default(true);
        match event_loop.create_window(attributes) {
            Ok(window) => {
                window.request_redraw();
                self.window = Some(window);
            }
            Err(error) => panic!("failed to create the browser canvas window: {error}"),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_none_or(|window| window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if self.lifecycle.transition(LifecycleEvent::ExitRequested) == LifecycleAction::Exit
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {}
            _ => {}
        }
    }
}

/// Start the browser frontend without emulating a never-returning native loop.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let event_loop = EventLoop::new().map_err(|error| JsValue::from_str(&error.to_string()))?;
    event_loop.spawn_app(BrowserFrontend::default());
    Ok(())
}
