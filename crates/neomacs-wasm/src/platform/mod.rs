//! Browser canvas and event-loop adapter.

use std::cell::RefCell;
use std::future::poll_fn;
use std::rc::Rc;

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use neomacs_display_protocol::FrameGlyphBuffer;
use neomacs_display_protocol::{FrameDisplayState, SealedFramePresentation};
use neomacs_layout_engine::bootstrap_frame::PortableBootstrapFrameBuilder;
use neomacs_wgpu_runtime::{
    PresentationOutcome, SurfaceCursorVisibility, SurfaceFrameRenderer, SurfaceWindow,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::WindowAttributesWeb;
use winit::window::{WindowAttributes, WindowId};

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
    #[wasm_bindgen(js_name = setTimeout)]
    fn set_timeout(callback: &JsValue, milliseconds: i32) -> i32;
    #[wasm_bindgen(js_name = clearTimeout)]
    fn clear_timeout(id: i32);
    #[wasm_bindgen(js_namespace = document, js_name = hasFocus)]
    fn document_has_focus() -> bool;
}

thread_local! {
    static PRESENTATION_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static ANIMATION_TIMER: RefCell<Option<i32>> = const { RefCell::new(None) };
    static ANIMATION_CALLBACK: Closure<dyn FnMut()> = Closure::new(|| {
        ANIMATION_TIMER.with(|slot| *slot.borrow_mut() = None);
        WORKER_WINDOW.with(|slot| {
            if let Some(window) = slot.borrow().as_ref() { window.request_redraw(); }
        });
    });
    static POINTER_FRONTEND: RefCell<std::rc::Weak<RefCell<Option<PresentedFrontend>>>> = RefCell::new(std::rc::Weak::new());
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
    displayed: Option<Rc<FrameGlyphBuffer>>,
    renderer: SurfaceFrameRenderer,
    bootstrap: PortableBootstrapFrameBuilder,
    frame: Option<BrowserPresentationFrame>,
}

enum BrowserPresentationFrame {
    Bootstrap(FrameGlyphBuffer),
    Editor(Rc<FrameGlyphBuffer>),
}

impl BrowserPresentationFrame {
    fn glyphs(&self) -> &FrameGlyphBuffer {
        match self {
            Self::Bootstrap(frame) => frame,
            Self::Editor(frame) => frame,
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

    const fn cursor_visibility(&self) -> SurfaceCursorVisibility {
        match self {
            Self::Bootstrap(_) => SurfaceCursorVisibility::Hidden,
            Self::Editor(_) => SurfaceCursorVisibility::Visible,
        }
    }
}

impl PresentedFrontend {
    fn new(renderer: SurfaceFrameRenderer) -> Result<Self, BrowserPresentationFailure> {
        let mut this = Self {
            displayed: None,
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
        // Keep displayed content and pointer hits coherent while the Worker
        // prepares new geometry. SurfaceFrameRenderer clips the old frame.
        if matches!(self.frame, Some(BrowserPresentationFrame::Editor(_))) {
            return Ok(());
        }
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
        let presented = Rc::new(RefCell::new(None));
        POINTER_FRONTEND.with(|slot| *slot.borrow_mut() = Rc::downgrade(&presented));
        Self {
            lifecycle: FrontendLifecycle::new(),
            window: None,
            presented,
        }
    }
}

/// Resolve a browser pointer against the immutable frame currently displayed.
/// CBOR carries 64-bit source/presentation identities losslessly through JS.
#[wasm_bindgen]
pub fn browser_pointer_input(
    x: f32,
    y: f32,
    button: u32,
    pressed: bool,
    modifiers: u32,
) -> Result<Vec<u8>, JsValue> {
    use neomacs_display_protocol::geometry::{GeometryPoint, LogicalPixels, RootSurfaceSpace};
    use neomacs_display_protocol::interaction_projection::InteractionProjection;
    use neomacs_display_protocol::{
        PointerAction, PointerPosition, PointerTarget, PositionedPointerInput, PresentedHitQuery,
    };
    if !x.is_finite() || !y.is_finite() || button > 5 {
        return Err(JsValue::from_str("invalid browser pointer"));
    }
    POINTER_FRONTEND.with(|slot| {
        let Some(frontend) = slot.borrow().upgrade() else {
            return Ok(Vec::new());
        };
        let state = frontend.borrow();
        let Some(frame) = state.as_ref().and_then(|state| state.displayed.as_ref()) else {
            return Ok(Vec::new());
        };
        // The portable compositor draws settled panes at their logical
        // top-left positions (no layout-morph transform). Name that witness
        // explicitly, as the native compositor does for settled frames.
        let projection = InteractionProjection::settled(frame.presentation_id);
        let surface_point = GeometryPoint::<RootSurfaceSpace, LogicalPixels>::from_px(x, y)
            .map_err(|error| JsValue::from_str(&format!("{error:?}")))?;
        let Some(point) = projection.map(surface_point) else {
            return Ok(Vec::new());
        };
        let hit = frame
            .resolve_presented_hit(PresentedHitQuery::new(point))
            .map_err(|error| JsValue::from_str(&format!("{error:?}")))?
            .and_then(|hit| hit.semantic());
        let input = PositionedPointerInput {
            position: PointerPosition {
                x,
                y,
                target_frame_id: frame.frame_placement.frame().get(),
            },
            target: PointerTarget::Presented {
                presentation: frame.presentation_id.get(),
                hit,
            },
            action: if button == 0 {
                PointerAction::Move { modifiers }
            } else {
                PointerAction::Button {
                    button,
                    pressed,
                    modifiers,
                }
            },
        };
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&input, &mut bytes)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(bytes)
    })
}

impl ApplicationHandler for BrowserFrontend {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.lifecycle.transition(LifecycleEvent::Resumed) != LifecycleAction::CreateFrontend {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title("Neomacs")
            .with_platform_attributes(Box::new(
                WindowAttributesWeb::default()
                    .with_append(true)
                    .with_focusable(true)
                    .with_prevent_default(true),
            ));
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = SurfaceWindow::from(window);
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
        event_loop: &dyn ActiveEventLoop,
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
            WindowEvent::Focused(focused) => {
                let _ = focused; // Canvas focus differs from hidden-text-input focus.
                schedule_animation(None);
                if document_has_focus()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            WindowEvent::CloseRequested => {
                if self.lifecycle.transition(LifecycleEvent::ExitRequested) == LifecycleAction::Exit
                {
                    event_loop.exit();
                }
            }
            WindowEvent::SurfaceResized(size) => {
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
                let size = self
                    .window
                    .as_ref()
                    .expect("validated window")
                    .surface_size();
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
                    presented.frame = Some(BrowserPresentationFrame::Editor(Rc::new(frame)));
                }
                let Some(frame) = presented.frame.as_ref() else {
                    return;
                };
                let provenance = frame.provenance();
                let cursor_visibility = frame.cursor_visibility();
                let outcome = match presented
                    .renderer
                    .present_frame(frame.glyphs(), cursor_visibility)
                {
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
                schedule_animation(
                    if document_has_focus() && matches!(outcome, PresentationOutcome::Presented) {
                        presented
                            .renderer
                            .animation_interval(frame.glyphs(), cursor_visibility)
                    } else {
                        None
                    },
                );
                if matches!(outcome, PresentationOutcome::Presented)
                    && let BrowserPresentationFrame::Editor(frame) = frame
                    && presented
                        .displayed
                        .as_ref()
                        .is_none_or(|old| old.presentation_id != frame.presentation_id)
                {
                    let frame = Rc::clone(frame);
                    let id = frame.presentation_id.get().to_string();
                    let target = frame.frame_placement.frame().get().to_string();
                    presented.displayed = Some(frame);
                    PRESENTATION_CALLBACK.with(|slot| {
                        if let Some(callback) = slot.borrow().as_ref() {
                            if let Err(error) = callback.call2(
                                &JsValue::NULL,
                                &JsValue::from_str(&id),
                                &JsValue::from_str(&target),
                            ) {
                                browser_console_error(&format!(
                                    "presentation feedback failed: {error:?}"
                                ));
                            }
                        }
                    });
                }
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

#[wasm_bindgen]
pub fn set_presentation_callback(callback: js_sys::Function) {
    PRESENTATION_CALLBACK.with(|slot| *slot.borrow_mut() = Some(callback));
}

fn schedule_animation(interval: Option<std::time::Duration>) {
    ANIMATION_TIMER.with(|slot| {
        if let Some(id) = slot.borrow_mut().take() {
            clear_timeout(id);
        }
        if let Some(interval) = interval {
            let id = ANIMATION_CALLBACK.with(|callback| {
                set_timeout(callback.as_ref(), interval.as_millis().max(1) as i32)
            });
            *slot.borrow_mut() = Some(id);
        }
    });
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
    // On web, winit 0.31 registers the application and returns immediately.
    event_loop
        .run_app(BrowserFrontend::default())
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    Ok(())
}
