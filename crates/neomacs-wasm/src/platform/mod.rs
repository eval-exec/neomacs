//! Browser canvas and event-loop adapter.

use std::cell::RefCell;
use std::rc::Rc;

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use neomacs_wgpu_runtime::{SurfaceClearColor, SurfaceRuntime, SurfaceWindow};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowId};

struct BrowserFrontend {
    lifecycle: FrontendLifecycle,
    window: Option<SurfaceWindow>,
    surface: Rc<RefCell<Option<SurfaceRuntime>>>,
}

const INITIAL_CLEAR_COLOR: SurfaceClearColor = SurfaceClearColor::rgb(0.055, 0.067, 0.090);

impl Default for BrowserFrontend {
    fn default() -> Self {
        Self {
            lifecycle: FrontendLifecycle::new(),
            window: None,
            surface: Rc::new(RefCell::new(None)),
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
                let window = SurfaceWindow::new(window);
                let display = event_loop.owned_display_handle();
                let surface_slot = Rc::clone(&self.surface);
                let surface_window = window.clone();
                spawn_local(async move {
                    let surface = SurfaceRuntime::new(display, surface_window.clone())
                        .await
                        .unwrap_or_else(|error| {
                            panic!("failed to initialize browser GPU presentation: {error}")
                        });
                    *surface_slot.borrow_mut() = Some(surface);
                    surface_window.request_redraw();
                });
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
            WindowEvent::Resized(size) => {
                if let Some(surface) = self.surface.borrow_mut().as_mut() {
                    surface.resize_physical(size.width, size.height);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let mut surface = self.surface.borrow_mut();
                let Some(surface) = surface.as_mut() else {
                    return;
                };
                let outcome = surface
                    .present_clear(INITIAL_CLEAR_COLOR)
                    .unwrap_or_else(|error| panic!("browser GPU presentation failed: {error}"));
                if outcome.should_request_redraw()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
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
