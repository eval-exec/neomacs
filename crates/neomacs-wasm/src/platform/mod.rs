//! Browser canvas and event-loop adapter.

use std::cell::RefCell;
use std::rc::Rc;

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use neomacs_display_protocol::FrameGlyphBuffer;
use neomacs_display_protocol::{FrameDisplayState, SealedFramePresentation};
use neomacs_layout_engine::bootstrap_frame::PortableBootstrapFrameBuilder;
use neomacs_wgpu_runtime::{SurfaceFrameRenderer, SurfaceWindow};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowId};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn browser_monotonic_time_milliseconds() -> f64;
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn browser_wall_time_milliseconds() -> f64;
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn browser_console_error(message: &str);
}

thread_local! {
    static WORKER_FRAME: RefCell<Option<FrameGlyphBuffer>> = const { RefCell::new(None) };
    static WORKER_WINDOW: RefCell<Option<SurfaceWindow>> = const { RefCell::new(None) };
}

struct BrowserFrontend {
    lifecycle: FrontendLifecycle,
    window: Option<SurfaceWindow>,
    presented: Rc<RefCell<Option<PresentedFrontend>>>,
}

struct PresentedFrontend {
    renderer: SurfaceFrameRenderer,
    bootstrap: PortableBootstrapFrameBuilder,
    frame: Option<FrameGlyphBuffer>,
}

impl PresentedFrontend {
    fn new(renderer: SurfaceFrameRenderer) -> Self {
        let mut this = Self {
            renderer,
            bootstrap: PortableBootstrapFrameBuilder::new(),
            frame: None,
        };
        this.resize_frame();
        this
    }

    fn resize_physical(&mut self, width: u32, height: u32) {
        self.renderer.resize_physical(width, height);
        self.resize_frame();
    }

    fn resize_frame(&mut self) {
        let size = self
            .renderer
            .logical_size()
            .unwrap_or_else(|error| panic!("invalid browser surface geometry: {error}"));
        self.frame = size.map(|size| {
            self.bootstrap
                .build(size)
                .unwrap_or_else(|error| panic!("failed to build browser initial frame: {error}"))
        });
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
                WORKER_WINDOW.with(|slot| *slot.borrow_mut() = self.window.clone());
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
                if let Some(frame) = WORKER_FRAME.with(|slot| slot.borrow_mut().take()) {
                    presented.frame = Some(frame);
                }
                let Some(frame) = presented.frame.as_ref() else {
                    return;
                };
                let outcome = presented
                    .renderer
                    .present_frame(frame)
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

/// Validate and install one evaluator presentation transferred by the editor
/// Worker. The returned receipt is sent back through the typed input protocol.
#[wasm_bindgen]
pub struct WorkerPresentationReceipt {
    presentation: String,
    target: String,
}

#[wasm_bindgen]
impl WorkerPresentationReceipt {
    #[wasm_bindgen(getter)]
    pub fn presentation(&self) -> String {
        self.presentation.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn target(&self) -> String {
        self.target.clone()
    }
}

/// Return the protocol version compiled into the Rust browser boundary.
#[wasm_bindgen]
pub fn worker_protocol_version() -> u16 {
    neomacs_wasm_protocol::WORKER_PROTOCOL_VERSION
}

/// Validate and install one evaluator presentation transferred by the editor
/// Worker. Its typed receipt keeps 64-bit identities lossless in JavaScript.
#[wasm_bindgen]
pub fn install_worker_presentation(bytes: &[u8]) -> Result<WorkerPresentationReceipt, JsValue> {
    let state: FrameDisplayState = ciborium::de::from_reader(bytes)
        .map_err(|error| JsValue::from_str(&format!("invalid Worker presentation: {error}")))?;
    let sealed = SealedFramePresentation::seal(state).map_err(|error| {
        JsValue::from_str(&format!("unsealable Worker presentation: {error:?}"))
    })?;
    let presentation = sealed.presentation().get();
    let target = sealed.frame_placement.frame().get();
    WORKER_FRAME.with(|slot| *slot.borrow_mut() = Some(sealed.materialize()));
    WORKER_WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.request_redraw();
        }
    });
    Ok(WorkerPresentationReceipt {
        presentation: presentation.to_string(),
        target: target.to_string(),
    })
}

/// Start the browser frontend without emulating a never-returning native loop.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(|panic| {
        browser_console_error(&format!("Neomacs frontend panicked: {panic}"));
    }));
    neomacs_host_runtime::time::BrowserClocks::new(
        browser_monotonic_time_milliseconds,
        browser_wall_time_milliseconds,
    )
    .install()
    .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let event_loop = EventLoop::new().map_err(|error| JsValue::from_str(&error.to_string()))?;
    event_loop.spawn_app(BrowserFrontend::default());
    Ok(())
}
