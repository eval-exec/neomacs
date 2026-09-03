//! Browser canvas and event-loop adapter.

use std::cell::RefCell;
use std::future::poll_fn;
use std::rc::Rc;

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use neomacs_display_protocol::FrameGlyphBuffer;
use neomacs_display_protocol::{FrameDisplayState, SealedFramePresentation};
use neomacs_layout_engine::bootstrap_frame::PortableBootstrapFrameBuilder;
use neomacs_wgpu_runtime::{PresentationOutcome, SurfaceFrameRenderer, SurfaceWindow};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowId};

use crate::presentation_readiness::{
    BrowserFrameProvenance, BrowserPresentationAttempt, BrowserPresentationFailure,
    BrowserPresentationId, FirstEditorPresentationLatch,
};

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
    static FIRST_EDITOR_PRESENTATION: RefCell<FirstEditorPresentationLatch> =
        RefCell::new(FirstEditorPresentationLatch::default());
}

struct BrowserFrontend {
    lifecycle: FrontendLifecycle,
    window: Option<SurfaceWindow>,
    presented: Rc<RefCell<Option<PresentedFrontend>>>,
}

struct PresentedFrontend {
    renderer: SurfaceFrameRenderer,
    bootstrap: PortableBootstrapFrameBuilder,
    frame: Option<BrowserPresentationFrame>,
}

enum BrowserPresentationFrame {
    Bootstrap(FrameGlyphBuffer),
    Editor(FrameGlyphBuffer),
}

impl BrowserPresentationFrame {
    fn glyphs(&self) -> &FrameGlyphBuffer {
        match self {
            Self::Bootstrap(frame) | Self::Editor(frame) => frame,
        }
    }

    fn provenance(&self) -> BrowserFrameProvenance {
        match self {
            Self::Bootstrap(_) => BrowserFrameProvenance::Bootstrap,
            Self::Editor(frame) => BrowserFrameProvenance::Editor(BrowserPresentationId::new(
                frame.presentation_id.get(),
            )),
        }
    }
}

impl PresentedFrontend {
    fn new(renderer: SurfaceFrameRenderer) -> Result<Self, BrowserPresentationFailure> {
        let mut this = Self {
            renderer,
            bootstrap: PortableBootstrapFrameBuilder::new(),
            frame: None,
        };
        this.resize_frame()?;
        Ok(this)
    }

    fn resize_physical(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<(), BrowserPresentationFailure> {
        self.renderer.resize_physical(width, height);
        self.resize_frame()
    }

    fn resize_frame(&mut self) -> Result<(), BrowserPresentationFailure> {
        let size = self
            .renderer
            .logical_size()
            .map_err(|error| BrowserPresentationFailure::SurfaceGeometry(error.to_string()))?;
        self.frame = size
            .map(|size| {
                self.bootstrap
                    .build(size)
                    .map(BrowserPresentationFrame::Bootstrap)
                    .map_err(|error| BrowserPresentationFailure::BootstrapFrame(error.to_string()))
            })
            .transpose()?;
        Ok(())
    }
}

fn report_presentation_failure(failure: BrowserPresentationFailure) {
    browser_console_error(&failure.to_string());
    FIRST_EDITOR_PRESENTATION.with(|latch| latch.borrow_mut().fail(failure));
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
                    match SurfaceFrameRenderer::new(display, surface_window.clone()).await {
                        Ok(renderer) => match PresentedFrontend::new(renderer) {
                            Ok(presented) => {
                                *presented_slot.borrow_mut() = Some(presented);
                                surface_window.request_redraw();
                            }
                            Err(failure) => report_presentation_failure(failure),
                        },
                        Err(error) => report_presentation_failure(
                            BrowserPresentationFailure::RendererInitialization(error.to_string()),
                        ),
                    }
                });
                self.window = Some(window);
                WORKER_WINDOW.with(|slot| *slot.borrow_mut() = self.window.clone());
            }
            Err(error) => {
                report_presentation_failure(BrowserPresentationFailure::WindowCreation(
                    error.to_string(),
                ));
                event_loop.exit();
            }
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
                    if let Err(failure) = presented.resize_physical(size.width, size.height) {
                        report_presentation_failure(failure);
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.as_ref().expect("validated window").inner_size();
                if let Some(presented) = self.presented.borrow_mut().as_mut() {
                    let resized = presented
                        .renderer
                        .set_scale_factor(scale_factor)
                        .map_err(|error| {
                            BrowserPresentationFailure::DisplayScale(error.to_string())
                        })
                        .and_then(|()| presented.resize_physical(size.width, size.height));
                    if let Err(failure) = resized {
                        report_presentation_failure(failure);
                    }
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
                    presented.frame = Some(BrowserPresentationFrame::Editor(frame));
                }
                let Some(frame) = presented.frame.as_ref() else {
                    return;
                };
                let provenance = frame.provenance();
                let outcome = match presented.renderer.present_frame(frame.glyphs()) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        report_presentation_failure(BrowserPresentationFailure::Rendering(
                            error.to_string(),
                        ));
                        return;
                    }
                };
                let attempt = match outcome {
                    PresentationOutcome::Presented => BrowserPresentationAttempt::Presented,
                    PresentationOutcome::Skipped(_) => BrowserPresentationAttempt::Skipped,
                };
                FIRST_EDITOR_PRESENTATION
                    .with(|latch| latch.borrow_mut().observe(provenance, attempt));
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

/// Resolve only after an evaluator-owned frame has reached the browser surface.
#[wasm_bindgen]
pub async fn wait_for_first_editor_presentation() -> Result<String, JsValue> {
    poll_fn(|context| FIRST_EDITOR_PRESENTATION.with(|latch| latch.borrow_mut().poll(context)))
        .await
        .map(|presentation| presentation.get().to_string())
        .map_err(|failure| JsValue::from_str(&failure.to_string()))
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
    FIRST_EDITOR_PRESENTATION.with(|latch| *latch.borrow_mut() = Default::default());
    let event_loop = EventLoop::new().map_err(|error| JsValue::from_str(&error.to_string()))?;
    event_loop.spawn_app(BrowserFrontend::default());
    Ok(())
}
