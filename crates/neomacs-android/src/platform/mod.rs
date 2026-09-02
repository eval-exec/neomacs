//! Android Activity and window lifecycle adapter.

use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent};
use neomacs_display_protocol::FrameGlyphBuffer;
use neomacs_layout_engine::bootstrap_frame::PortableBootstrapFrameBuilder;
use neomacs_wgpu_runtime::{SurfaceFrameRenderer, SurfaceWindow};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{Window, WindowId};

struct AndroidFrontend {
    lifecycle: FrontendLifecycle,
    window: Option<SurfaceWindow>,
    presented: Option<PresentedFrontend>,
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
            .unwrap_or_else(|error| panic!("invalid Android surface geometry: {error}"));
        self.frame = size.map(|size| {
            self.bootstrap
                .build(size)
                .unwrap_or_else(|error| panic!("failed to build Android initial frame: {error}"))
        });
    }
}

impl Default for AndroidFrontend {
    fn default() -> Self {
        Self {
            lifecycle: FrontendLifecycle::new(),
            window: None,
            presented: None,
        }
    }
}

impl ApplicationHandler for AndroidFrontend {
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
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Android may destroy the native window while retaining the Activity.
        // The renderer will eventually own surface teardown at this point; the
        // tracer bullet drops the window and recreates it on the next resume.
        if self.lifecycle.transition(LifecycleEvent::Suspended) == LifecycleAction::DestroyFrontend
        {
            self.presented = None;
            self.window = None;
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
                if let Some(presented) = self.presented.as_mut() {
                    presented.resize_physical(size.width, size.height);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.as_ref().expect("validated window").inner_size();
                if let Some(presented) = self.presented.as_mut() {
                    presented
                        .renderer
                        .set_scale_factor(scale_factor)
                        .unwrap_or_else(|error| panic!("invalid Android display scale: {error}"));
                    presented.resize_physical(size.width, size.height);
                }
                self.window
                    .as_ref()
                    .expect("validated window")
                    .request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let Some(presented) = self.presented.as_mut() else {
                    return;
                };
                let Some(frame) = presented.frame.as_ref() else {
                    return;
                };
                let outcome = presented
                    .renderer
                    .present_frame(frame)
                    .unwrap_or_else(|error| panic!("Android GPU presentation failed: {error}"));
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

/// Android Activity main-loop entrypoint required by `android-activity`.
///
/// This symbol uses Rust's ABI because `android-activity` declares it through
/// `extern "Rust"`. It can be invoked again after Activity recreation, so all
/// Activity-owned state remains local to this call.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let mut builder = EventLoop::builder();
    builder.with_android_app(app);
    let event_loop = builder
        .build()
        .unwrap_or_else(|error| panic!("failed to create the Android event loop: {error}"));

    let mut frontend = AndroidFrontend::default();
    event_loop
        .run_app(&mut frontend)
        .unwrap_or_else(|error| panic!("Android event loop failed: {error}"));
}
