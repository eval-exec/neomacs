//! Android Activity, editor-session, and window lifecycle adapter.

mod evaluator;
mod presentation;

use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendLogicalExtent, FrontendScaleFactor, FrontendViewport,
};
use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use neomacs_app::session::{
    FrontendFrameInbox, FrontendFrameReceive, FrontendInputPort, NativeEditorWorker,
    NativeEditorWorkerEvent,
};
use neomacs_wgpu_runtime::{SurfaceFrameRenderer, SurfaceWindow, WinitFrontendInput};
use presentation::PresentedFrontend;
use winit::application::ApplicationHandler;
use winit::event::{Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{Window, WindowId};

struct AndroidFrontend {
    app: AndroidApp,
    worker_events: EventLoopProxy<NativeEditorWorkerEvent>,
    lifecycle: FrontendLifecycle,
    window: Option<SurfaceWindow>,
    presented: Option<PresentedFrontend>,
    worker: Option<NativeEditorWorker>,
    input: Option<FrontendInputPort>,
    frames: Option<FrontendFrameInbox>,
    input_translation: WinitFrontendInput,
    target: FrontendFrameId,
    close_pending: bool,
}

impl AndroidFrontend {
    fn new(app: AndroidApp, worker_events: EventLoopProxy<NativeEditorWorkerEvent>) -> Self {
        Self {
            app,
            worker_events,
            lifecycle: FrontendLifecycle::new(),
            window: None,
            presented: None,
            worker: None,
            input: None,
            frames: None,
            input_translation: WinitFrontendInput::default(),
            target: FrontendFrameId::PRIMARY,
            close_pending: false,
        }
    }

    fn start_worker(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let Some(logical_extent) = self.presented_logical_extent() else {
            return;
        };
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let device_scale = FrontendScaleFactor::new(window.scale_factor())
            .expect("winit supplied an invalid Android scale factor");
        let proxy = self.worker_events.clone();
        self.worker = Some(
            evaluator::spawn(
                self.app.clone(),
                logical_extent,
                device_scale,
                move |event| {
                    let _ = proxy.send_event(event);
                },
            )
            .unwrap_or_else(|error| panic!("failed to spawn Android evaluator: {error}")),
        );
    }

    fn presented_logical_extent(&self) -> Option<FrontendLogicalExtent> {
        let size = self.presented.as_ref()?.logical_size().ok().flatten()?;
        Some(FrontendLogicalExtent::new(
            size.width().round().max(1.0) as u32,
            size.height().round().max(1.0) as u32,
        ))
    }

    fn submit(&mut self, event: FrontendEvent) {
        if self
            .input
            .as_ref()
            .is_some_and(|input| input.submit(&event).is_err())
        {
            self.input = None;
        }
    }

    fn submit_viewport(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(logical_extent) = self.presented_logical_extent() else {
            return;
        };
        let viewport = FrontendViewport::new(logical_extent, window.scale_factor(), self.target)
            .expect("winit supplied an invalid Android scale factor");
        self.submit(FrontendEvent::ViewportChanged(viewport));
    }

    fn receive_latest_frame(&mut self) {
        let receive = match self.frames.as_mut() {
            Some(frames) => frames.try_latest(),
            None => return,
        };
        match receive {
            FrontendFrameReceive::Empty => {}
            FrontendFrameReceive::Disconnected => self.frames = None,
            FrontendFrameReceive::Frame(pending) => {
                let Some(presented) = self.presented.as_mut() else {
                    // Dropping the pending guard reports a discard. A viewport
                    // event on resume asks the evaluator for a fresh revision.
                    return;
                };
                match presented.install(pending) {
                    Ok(target) => {
                        self.target = target;
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                    }
                    Err(_) => self.input = None,
                }
            }
        }
    }

    fn finish_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .unwrap_or_else(|_| panic!("Android evaluator worker panicked"));
        }
    }

    fn request_exit(&mut self, event_loop: &ActiveEventLoop) {
        if self.lifecycle.transition(LifecycleEvent::ExitRequested) == LifecycleAction::Exit {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<NativeEditorWorkerEvent> for AndroidFrontend {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.lifecycle.transition(LifecycleEvent::Resumed) != LifecycleAction::CreateFrontend {
            return;
        }

        let attributes = Window::default_attributes().with_title("Neomacs");
        let window = SurfaceWindow::new(
            event_loop
                .create_window(attributes)
                .unwrap_or_else(|error| panic!("failed to create the Android window: {error}")),
        );
        let renderer = pollster::block_on(SurfaceFrameRenderer::new(
            event_loop.owned_display_handle(),
            window.clone(),
        ))
        .unwrap_or_else(|error| panic!("failed to initialize Android GPU presentation: {error}"));

        window.request_redraw();
        self.window = Some(window);
        self.presented = Some(PresentedFrontend::new(renderer));
        self.start_worker();
        self.submit_viewport();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // The evaluator and transport survive Activity surface loss. Dropping
        // presentation retires its active frame before the Android window is
        // released; resume creates a new surface and requests a fresh frame.
        if self.lifecycle.transition(LifecycleEvent::Suspended) == LifecycleAction::DestroyFrontend
        {
            self.presented = None;
            self.window = None;
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: NativeEditorWorkerEvent) {
        match event {
            NativeEditorWorkerEvent::Started(frontend) => {
                let (input, frames) = frontend.split();
                self.input = Some(input);
                self.frames = Some(frames);
                self.submit_viewport();
                if self.close_pending {
                    self.submit(FrontendEvent::CloseRequested {
                        target: self.target,
                    });
                }
            }
            NativeEditorWorkerEvent::FramesReady => self.receive_latest_frame(),
            NativeEditorWorkerEvent::StartupFailed(error) => {
                eprintln!("Neomacs Android startup failed: {error}");
                self.finish_worker();
                self.request_exit(event_loop);
            }
            NativeEditorWorkerEvent::Exited(exit) => {
                if let Some(error) = exit.command_loop_error() {
                    eprintln!("Neomacs Android command loop failed: {error}");
                }
                self.finish_worker();
                self.request_exit(event_loop);
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
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
                self.close_pending = true;
                self.submit(FrontendEvent::CloseRequested {
                    target: self.target,
                });
            }
            WindowEvent::Resized(size) => {
                if let Some(presented) = self.presented.as_mut() {
                    presented.resize_physical(size.width, size.height);
                }
                self.start_worker();
                self.submit_viewport();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.as_ref().expect("validated window").inner_size();
                if let Some(presented) = self.presented.as_mut() {
                    presented
                        .set_scale_factor(scale_factor)
                        .unwrap_or_else(|error| panic!("invalid Android display scale: {error}"));
                    presented.resize_physical(size.width, size.height);
                }
                self.start_worker();
                self.submit_viewport();
                self.window
                    .as_ref()
                    .expect("validated window")
                    .request_redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input_translation.set_modifiers(modifiers.state());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(event) = self.input_translation.translate_key(
                    &event.logical_key,
                    event.text.as_deref(),
                    event.state,
                    self.target,
                ) {
                    self.submit(event);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(event) = self.input_translation.committed_text(&text, self.target) {
                    self.submit(event);
                }
            }
            WindowEvent::Focused(focused) => self.submit(FrontendEvent::FocusChanged {
                focused,
                target: self.target,
            }),
            WindowEvent::RedrawRequested => {
                let Some(presented) = self.presented.as_mut() else {
                    return;
                };
                let outcome = presented
                    .present()
                    .unwrap_or_else(|error| panic!("Android GPU presentation failed: {error}"));
                if outcome.is_some_and(|outcome| outcome.should_request_redraw())
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Android Activity main-loop entrypoint required by `android-activity`.
///
/// This symbol uses Rust's ABI because `android-activity` declares it through
/// `extern "Rust"`. It can be invoked again after Activity recreation, so all
/// Activity-owned state remains local to this call.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let mut builder = EventLoop::<NativeEditorWorkerEvent>::with_user_event();
    builder.with_android_app(app.clone());
    let event_loop = builder
        .build()
        .unwrap_or_else(|error| panic!("failed to create the Android event loop: {error}"));
    let worker_events = event_loop.create_proxy();

    let mut frontend = AndroidFrontend::new(app, worker_events);
    event_loop
        .run_app(&mut frontend)
        .unwrap_or_else(|error| panic!("Android event loop failed: {error}"));
}
