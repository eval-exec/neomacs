//! Browser canvas and event-loop adapter.

use std::cell::RefCell;
use std::rc::Rc;

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use neomacs_display_protocol::{Color, FrameGlyphBuffer};
use neomacs_wgpu_runtime::{SurfaceFrameRenderer, SurfaceWindow};
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
    presented: Rc<RefCell<Option<PresentedFrontend>>>,
}

struct PresentedFrontend {
    renderer: SurfaceFrameRenderer,
    frame: FrameGlyphBuffer,
}

impl PresentedFrontend {
    fn new(renderer: SurfaceFrameRenderer) -> Self {
        let mut this = Self {
            renderer,
            frame: FrameGlyphBuffer::new(),
        };
        this.resize_frame();
        this
    }

    fn resize_physical(&mut self, width: u32, height: u32) {
        self.renderer.resize_physical(width, height);
        self.resize_frame();
    }

    fn resize_frame(&mut self) {
        let Some(size) = self
            .renderer
            .logical_size()
            .unwrap_or_else(|error| panic!("invalid browser surface geometry: {error}"))
        else {
            return;
        };
        self.frame.width = size.width();
        self.frame.height = size.height();
        self.frame.background = Color::rgb(0.055, 0.067, 0.090);
    }
}

impl Default for BrowserFrontend {
    fn default() -> Self {
        Self {
            lifecycle: FrontendLifecycle::new(),
            window: None,
            presented: Rc::new(RefCell::new(None)),
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
                let presented_slot = Rc::clone(&self.presented);
                let surface_window = window.clone();
                spawn_local(async move {
                    let renderer = SurfaceFrameRenderer::new(display, surface_window.clone())
                        .await
                        .unwrap_or_else(|error| {
                            panic!("failed to initialize browser GPU presentation: {error}")
                        });
                    *presented_slot.borrow_mut() = Some(PresentedFrontend::new(renderer));
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
                if let Some(presented) = self.presented.borrow_mut().as_mut() {
                    presented.resize_physical(size.width, size.height);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.as_ref().expect("validated window").inner_size();
                if let Some(presented) = self.presented.borrow_mut().as_mut() {
                    presented
                        .renderer
                        .set_scale_factor(scale_factor)
                        .unwrap_or_else(|error| panic!("invalid browser display scale: {error}"));
                    presented.resize_physical(size.width, size.height);
                }
                self.window
                    .as_ref()
                    .expect("validated window")
                    .request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let mut presented = self.presented.borrow_mut();
                let Some(presented) = presented.as_mut() else {
                    return;
                };
                let outcome = presented
                    .renderer
                    .present_frame(&presented.frame)
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
